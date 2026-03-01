(ns paxos-shim.core
  (:require [jepsen.checker :as checker]
            [jepsen.generator :as gen]
            [jepsen.tests :as tests]
            [knossos.model :as model]
            [jepsen.core :as jepsen]
            [jepsen.os :as os]
            [jepsen.db :as db]
            [paxos-shim.client :as paxos-client]
            [clj-http.client :as http])
  (:gen-class))


(def noop-remote
  (reify jepsen.control.core/Remote
    (connect [this node] this)
    (disconnect_BANG_ [this] nil)))

(defn paxos-test
  [opts]
  (merge tests/noop-test
         {:name      "paxos-lin-kv"
          :os        os/noop
          :db        db/noop
          :remote    noop-remote
          :concurrency 10 ; 10 parallel workers
          :client    (paxos-client/paxos-client)
          :checker   (checker/compose
                      {:linear (checker/linearizable {:model (model/register)
                                                      :algorithm :linearizable})})
          :generator (->> (fn [test process]
                            ;; This function is called for every single operation
                            ;; 50% chance for :read, 50% chance for :write
                            (if (< (rand) 0.5)
                              {:f :read}
                              {:f :write :value (rand-int 100)}))
                          (gen/stagger 1/5)    ; 200ms delay between ops per thread
                          (gen/time-limit 60)  ; Run for 60 seconds
                          (gen/clients))
}      ; Distribute among the 10 workers
         opts))


(defn -main [& args]
  (println "🛡️ Starting Jepsen Test...")
  (let [test (paxos-test {:nodes ["local-shim"]})]
    (try
      (let [result (jepsen/run! test)])
      (catch Exception e
        (println "❌ Jepsen Run Error:" (.getMessage e))
        (.printStackTrace e)))))
