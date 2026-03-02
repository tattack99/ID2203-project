(ns paxos-shim.core
  (:require [jepsen.checker :as checker]
            [jepsen.generator :as gen]
            [jepsen.tests :as tests]
            [knossos.model :as model]
            [jepsen.core :as jepsen]
            [jepsen.os :as os]
            [jepsen.db :as db]
            [jepsen.nemesis :as nemesis] ; Added nemesis require
            [paxos-shim.client :as paxos-client]
            [clj-http.client :as http])
  (:gen-class))

;; 1. Define the Remote first so paxos-test can see it
(def noop-remote
  (reify jepsen.control.core/Remote
    (connect [this node] this)
    (disconnect_BANG_ [this] nil)))

;; 2. Define the Test Configuration
(defn paxos-test
  [opts]
  (let [mode (:mode opts)]
    (merge tests/noop-test
           {:name      "paxos-lin-kv"
            :os        os/noop
            :db        db/noop
            :remote    noop-remote
            :concurrency 10
            :client    (paxos-client/paxos-client)
            :checker   (checker/compose
                        {:linear (checker/linearizable {:model (model/register)})})
            
            ;; Nemesis setup based on CLI mode
            :nemesis   (if (= mode "partition")
                         (nemesis/partition-random-halves)
                         nemesis/noop)
            
            :generator (->> (fn [t p] 
                              (if (< (rand) 0.5) 
                                {:f :read} 
                                {:f :write :value (rand-int 100)}))
                            (gen/stagger 1/5)
                            (gen/time-limit 60)
                            ;; Nemesis schedule
                            (gen/nemesis
                             (if (= mode "partition")
                               (cycle [(gen/sleep 5)
                                       {:type :info :f :start}
                                       (gen/sleep 5)
                                       {:type :info :f :stop}])
                               nil)) ; In 0.3.10, nil is the valid no-op
                            (gen/clients))}
           opts)))

;; 3. Main Entry Point
(defn -main [& args]
  (let [mode (or (first args) "check")]
    (println "🛡️ Starting Jepsen Test in mode:" mode)
    (let [test (paxos-test {:nodes ["local-shim"] :mode mode})]
      (try
        (let [result (jepsen/run! test)])
        (catch Exception e
          (println "❌ Jepsen Run Error:" (.getMessage e))
          (.printStackTrace e))))))
