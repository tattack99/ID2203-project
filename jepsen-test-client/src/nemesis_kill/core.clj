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

(defn start-server! [test node]
  (info "Nemesis: Restarting OmniPaxos on" node)
  ;; setsid + redirection ensures the process detaches from the Jepsen SSH session
  (c/exec :sh :-c "setsid /usr/local/bin/server > /app/logs/stdout.log 2>&1 &")
  :started)

(defn stop-server! [test node]
  (info "Nemesis: SIGKILL OmniPaxos on" node)
  ;; '|| true' ensures the test doesn't crash if the process is already dead
  (c/exec :sh :-c "pkill -9 server || true")
  :killed)

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
  (let [ssh-hosts ["127.0.0.2" "127.0.0.3" "127.0.0.4"]
        node-map  {"127.0.0.2" "http://localhost:3001" 
                   "127.0.0.3" "http://localhost:3002" 
                   "127.0.0.4" "http://localhost:3001"}]
    (merge tests/noop-test
           opts
           {:name      "omnipaxos-crash-recovery"
            :nodes     ssh-hosts
            :client    (->PaxosClient node-map)
            :nemesis   (nemesis/node-start-stopper 
                         (fn [test nodes] (rand-nth nodes)) 
                         start-server! 
                         stop-server!)
            :checker   (checker/compose 
                         {:linear (checker/linearizable {:model (model/register)})})
            :concurrency 10
            :generator (->> (gen/mix [(fn [_ _] {:f :read}) 
                          (fn [_ _] {:f :write :value (rand-int 100)})])
                (gen/stagger 1/10)
                (gen/nemesis
                  (gen/cycle [(gen/sleep 10)
                              {:type :info, :f :stop}
                              (gen/sleep 5)
                              {:type :info, :f :start}]))
                (gen/time-limit (:time-limit opts)))
})))


(defn -main
  [& args]
  (cli/run! (cli/single-test-cmd {:test-fn omnipaxos-test})
            (concat ["test"
                     "--nodes" "127.0.0.2,127.0.0.3,127.0.0.4"
                     "--username" "root"
                     "--password" "root"
                     "--time-limit" "60"
                     "--concurrency" "10"]
                    args)))
