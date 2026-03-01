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
    (disconnect_BANG_ [this] nil))) ; Matches the underscores Jepsen expects

(defn paxos-test
  [opts]
  (merge tests/noop-test
         {:name      "paxos-lin-kv"
          :os        os/noop
          :db        db/noop
          :remote    noop-remote
          :client    (paxos-client/paxos-client)
          :checker   (checker/compose
                      {:linear (checker/linearizable {:model (model/register)
                                                      :algorithm :linearizable})})
          :generator (->> (gen/mix [{:f :read} 
                                    (fn [test process] {:f :write, :value (rand-int 100)})])
                          (gen/stagger 1/10)
                          (gen/limit 20)
                          (gen/clients))}
         opts))

(defn -main [& args]
  (println "🛡️ Starting Jepsen Test...")
  (let [test (paxos-test {:nodes ["local-shim"]})]
    (try
      (let [result (jepsen/run! test)])
      (catch Exception e
        (println "❌ Jepsen Run Error:" (.getMessage e))
        (.printStackTrace e)))))
