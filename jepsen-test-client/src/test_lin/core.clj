(ns test-lin.core
  (:require [jepsen.checker :as checker]
            [jepsen.generator :as gen]
            [jepsen.tests :as tests]
            [knossos.model :as model]
            [jepsen.core :as jepsen]
            [jepsen.os :as os]
            [jepsen.db :as db]
            [test-lin.client :as paxos-client])
  (:gen-class))

(def noop-remote
  (reify jepsen.control.core/Remote
    (connect [this _] this)
    (disconnect_BANG_ [_] nil)))

(defn paxos-test
  [opts]
  (merge tests/noop-test
         {:name        "paxos-lin-kv"
          :os          os/noop
          :db          db/noop
          :remote      noop-remote
          :concurrency 10
          :client      (paxos-client/paxos-client)
          :checker     (checker/compose {:linear (checker/linearizable {:model (model/register)})})
          :generator   (->> (fn [_ _]
                              (if (< (rand) 0.5)
                                {:f :read}
                                {:f :write :value (rand-int 100)}))
                            (gen/stagger 1/5)
                            (gen/time-limit 60)
                            (gen/clients))}
         opts))

(defn -main [& _]
  (println "🛡️ Starting Jepsen Test...")
  (let [test (paxos-test {:nodes ["3001" "3002"]})]
    (try
      (jepsen/run! test)
      (catch Exception e
        (println "❌ Jepsen Run Error:" (.getMessage e))))))

