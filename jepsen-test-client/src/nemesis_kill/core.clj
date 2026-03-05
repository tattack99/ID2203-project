(ns nemesis-kill.core
  (:gen-class)
  (:require [clojure.tools.logging :refer [info]]
            [clj-http.client :as http]
            [jepsen [cli :as cli]
                    [checker :as checker]
                    [control :as c]
                    [db :as db]
                    [generator :as gen]
                    [nemesis :as nemesis]
                    [tests :as tests]
                    [client :as client]]
            [knossos.model :as model]))

(def node->id 
  {"127.0.0.2" 1 
   "127.0.0.3" 2 
   "127.0.0.4" 3})

(defn stop-server! [test node]
  (info node "Nemesis: SIGKILL OmniPaxos")
  (c/on-nodes test [node]
    (fn [t n]
      (try
        (c/su (c/exec :pkill :-9 :-f "server"))
        (catch Exception _ :already-stopped))))
  :killed)

(defn start-server! [test node]
  (info node "Nemesis: Starting OmniPaxos")
  (c/on-nodes test [node]
    (fn [t n]
      (c/su
        (c/exec :sh :-c
          (str "export RUST_BACKTRACE=1 && "
               "export RUST_LOG=debug && "
               "export SERVER_CONFIG_FILE=/app/server-config.toml && "
               "export CLUSTER_CONFIG_FILE=/app/cluster-config.toml && "
               "export OMNIPAXOS_NODE_ADDRS='s1:8000,s2:8000,s3:8000' && "
               "export OMNIPAXOS_LISTEN_ADDRESS=0.0.0.0 && "
               "export OMNIPAXOS_LISTEN_PORT=8000 && "
               "fuser -k 8000/tcp || true && "
               "sleep 1 && "
               "cd /app && "
               "setsid /usr/local/bin/server > /app/logs/nemesis.log 2>&1 &")))))
  :started)


(defrecord PaxosClient [node-map]
  client/Client
  (open! [this _ node] (assoc this :url (get node-map node)))
  (setup! [_ _])
  (invoke! [this _ op]
  (try
    (case (:f op)
      :read  (let [resp (http/get (str (:url this) "/get/jepsen-key") 
                                 {:conn-timeout 500 :socket-timeout 500}) 
                   b (:body resp)]
               (cond 
                 ;; IF THE SERVER SAYS IT'S NOT THE LEADER:
                 (.contains b "Not the leader") 
                 (assoc op :type :info :error "Not Leader")
                 
                 ;; IF THE KEY ISN'T THERE:
                 (or (empty? b) (= b "Key not found"))
                 (assoc op :type :ok :value nil)
                 
                 ;; SUCCESSFUL READ:
                 :else
                 (assoc op :type :ok :value (Integer/parseInt b))))
      
      :write (let [resp (http/post (str (:url this) "/put")
                                  {:form-params {:key "jepsen-key" :value (str (:value op))}
                                   :content-type :json 
                                   :conn-timeout 500 
                                   :socket-timeout 500})
                   b (:body resp)]
               (if (.contains b "Not the leader")
                 (assoc op :type :info :error "Not Leader")
                 (assoc op :type :ok))))
    (catch Exception e 
      ;; Network errors are also :info because we don't know if the write reached the log
      (assoc op :type :info :error (.getMessage e)))))
  (teardown! [_ _])
  (close! [_ _]))

(defn omnipaxos-test [opts]
  (let [ssh-hosts (keys node->id)
        node-map  {"127.0.0.2" "http://localhost:3001" 
                   "127.0.0.3" "http://localhost:3002" 
                   "127.0.0.4" "http://localhost:3003"}]
    (merge tests/noop-test
           opts
           {:name      "omnipaxos-crash-recovery"
            :nodes     ssh-hosts
            :db        db/noop
            :client    (->PaxosClient node-map)
            :nemesis   (nemesis/node-start-stopper 
                         (fn [test nodes] (rand-nth nodes)) 
                         start-server! 
                         stop-server!)
            :checker   (checker/compose 
                         {:linear (checker/linearizable {:model (model/register)})})
            :generator (->> (gen/mix [(fn [_ _] {:f :read}) 
                                      (fn [_ _] {:f :write :value (rand-int 100)})])
                (gen/stagger 1/10)
                (gen/nemesis
                  (gen/cycle [
                    (gen/sleep 5)
                    {:type :info, :f :stop}   ;; Kill 1 node (2/3 remain = Quorum OK)
                    (gen/sleep 30)            ;; Keep it dead long enough to force a leader change
                    {:type :info, :f :start}  ;; Bring it back
                    (gen/sleep 20)            ;; Wait for 'is_accept' to stabilize on the recovered node
                  ]))
                (gen/time-limit (:time-limit opts)))})))

(defn -main [& args]
  (cli/run! (cli/single-test-cmd {:test-fn omnipaxos-test}) args))
