(ns nemesis-recovery.core
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

;; SSH IPs → HTTP endpoints for the Jepsen client
(def nodes
  {"127.0.0.2" "http://localhost:3001"
   "127.0.0.3" "http://localhost:3002"
   "127.0.0.4" "http://localhost:3003"})

(def http-opts {:conn-timeout 5000 :socket-timeout 3000})

;; --- Nemesis: kill and restart a random server node ---

(defn stop-server! [test node]
  (info node "killing server")
  (c/on-nodes test [node]
    (fn [_ _] (c/su (try (c/exec :pkill :-9 :-f "server")
                         (catch Exception _ nil)))))
  :killed)

(defn start-server! [test node]
  (info node "starting server")
  (c/on-nodes test [node]
    (fn [_ _]
      (c/su
        (c/exec :sh :-c
          (str "export RUST_LOG=debug"
               " RUST_BACKTRACE=1"
               " SERVER_CONFIG_FILE=/app/server-config.toml"
               " CLUSTER_CONFIG_FILE=/app/cluster-config.toml"
               " OMNIPAXOS_NODE_ADDRS='s1:8000,s2:8000,s3:8000'"
               " OMNIPAXOS_LISTEN_ADDRESS=0.0.0.0"
               " OMNIPAXOS_LISTEN_PORT=8000 && "
               "fuser -k 8000/tcp || true && sleep 1 && "
               "cd /app && setsid /usr/local/bin/server > /app/logs/nemesis.log 2>&1 &")))))
  :started)

;; --- Jepsen client: read/write via HTTP ---

(defrecord PaxosClient [url]
  client/Client
  (open!    [this _ node] (assoc this :url (get nodes node)))
  (setup!   [_ _])
  (teardown![_ _])
  (close!   [_ _])
  (invoke!  [this _ op]
    (try
      (case (:f op)
        :read  (let [body (:body (http/get (str url "/get/jepsen-key") http-opts))]
                 (if (or (empty? body)
                         (= body "Key not found")
                         (.startsWith body "Error"))
                   (assoc op :type :fail :error body)
                   (assoc op :type :ok
                             :value (Integer/parseInt body))))
        :write (let [body (:body (http/post (str url "/put")
                                            (merge http-opts
                                                   {:form-params {:key "jepsen-key" :value (str (:value op))}
                                                    :content-type :json})))]
                 (if (and body (.startsWith body "Error"))
                   (assoc op :type :info :error body)
                   (assoc op :type :ok))))
      (catch Exception e
        (assoc op :type :info :error (.getMessage e))))))

;; --- Test definition ---
;; NOTE: Run build_run.sh first to wipe persistent state and restart
;; the cluster fresh. The db is noop because the cluster lifecycle is
;; managed by docker compose, not by Jepsen.

(defn omnipaxos-test [opts]
  (merge tests/noop-test opts
    {:name    "omnipaxos-recovery"
     :nodes   (keys nodes)
     :db      db/noop
     :client  (->PaxosClient nil)
     :nemesis (nemesis/node-start-stopper
                (fn [_ nodes] (rand-nth nodes))
                stop-server!
                start-server!)
     :checker (checker/compose
                {:linear (checker/linearizable {:model (model/register)})})
     :generator
     (->> (gen/mix [(fn [_ _] {:f :read})
                    (fn [_ _] {:f :write :value (rand-int 100)})])
          (gen/stagger 1/2)
          (gen/nemesis
            (gen/cycle [(gen/sleep 10)            ; let cluster stabilize and accept writes
                        {:type :info :f :stop}    ; kill 1 node (may be leader)
                        (gen/sleep 30)            ; wait for re-election + recovery
                        {:type :info :f :start}   ; restart the node
                        (gen/sleep 30)]))          ; wait for rejoin + catch-up
          (gen/time-limit (:time-limit opts)))}))

(defn -main [& args]
  (cli/run! (cli/single-test-cmd {:test-fn omnipaxos-test}) args))