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

(defn running? [node]
  (try
    (let [res (c/exec :sh :-c "pgrep -f /usr/local/bin/server" {:continue? true})]
      (and res (= 0 (:exit res))))
    (catch Exception _ false)))

(defn stop-server! [test node]
  (info node "Nemesis: SIGKILL OmniPaxos")
  (try
    ;; NOTICE: The {} map is OUTSIDE the vector arguments for :sh :-c
    (c/exec :sh :-c "pkill -9 -f /usr/local/bin/server || true" {:continue? true})
    :killed
    (catch Exception e
      (info node "Nemesis: Kill failed (likely already dead):" (.getMessage e))
      :killed)))

(defn start-server! [test node]
  (if (running? node)
    (do (info node "Nemesis: Already running.") :already-running)
    (let [id (get node->id node)]
      (info node "Nemesis: Starting OmniPaxos ID" id)
      (try
        (c/exec :sh :-c "cd /app && setsid /usr/local/bin/server > /app/logs/stdout.log 2>&1 &" {:continue? true})
        :started
        (catch Exception e
          (info node "Nemesis: Start failed:" (.getMessage e))
          :error)))))

(defrecord PaxosClient [node-map]
  client/Client
  (open! [this _ node] (assoc this :url (get node-map node)))
  (setup! [_ _])
  (invoke! [this _ op]
    (try
      (case (:f op)
        :read  (let [resp (http/get (str (:url this) "/get/jepsen-key") 
                                   {:conn-timeout 1000 :socket-timeout 1000}) 
                     b (:body resp)]
                 (assoc op :type :ok :value (when-not (or (empty? b) (= b "Key not found")) 
                                              (Integer/parseInt b))))
        :write (do (http/post (str (:url this) "/put")
                              {:form-params {:key "jepsen-key" :value (str (:value op))}
                               :content-type :json 
                               :conn-timeout 1000 
                               :socket-timeout 1000})
                   (assoc op :type :ok)))
      (catch Exception e 
        (assoc op :type :info :error (.getMessage e)))))
  (teardown! [_ _])
  (close! [_ _]))

(defn omnipaxos-test [opts]
  (let [ssh-hosts (keys node->id)
        node-map  {"127.0.0.2" "http://localhost:3001" 
                   "127.0.0.3" "http://localhost:3002" 
                   "127.0.0.4" "http://localhost:3001"}]
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
                  (gen/cycle [(gen/sleep 5)       ;; Initial Election Window
                              {:type :info, :f :stop}
                              (gen/sleep 5)        ;; Downtime
                              {:type :info, :f :start}
                              (gen/sleep 5)]))    ;; Recovery/Sync Window
                (gen/time-limit (:time-limit opts)))})))

(defn -main [& args]
  (cli/run! (cli/single-test-cmd {:test-fn omnipaxos-test}) args))
