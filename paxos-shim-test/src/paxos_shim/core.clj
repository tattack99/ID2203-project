(ns paxos-shim.core
  (:require [jepsen.checker :as checker]
            [jepsen.generator :as gen]
            [jepsen.tests :as tests]
            [jepsen.core :as jepsen]
            [jepsen.client :as client]
            [clj-http.client :as http]
            [cheshire.core :as json]
            [knossos.model :as model])
  (:gen-class))

(defrecord PaxosClient [node]
  client/Client
  (open! [this test node] (assoc this :node node))
  (setup! [this test])

  (invoke! [this test op]
    (let [url (:node this)
          [k v] (:value op)]
      (try
        (case (:f op)
          :read (let [resp (http/get (str url "/get/" k))]
                  (assoc op :type :ok :value (json/parse-string (:body resp))))
          :write (do (http/post (str url "/put")
                                {:content-type :json
                                 :body (json/generate-string {:key (str k) :value (str v)})})
                     (assoc op :type :ok)))
        (catch Exception e (assoc op :type :fail :error (.getMessage e))))))

  (teardown! [this test])
  (close! [this test]))

(defn paxos-test []
  (merge tests/noop-test
    {:name    "paxos-simple"
     :nodes   ["http://localhost:3001" "http://localhost:3002" "http://localhost:3003"]
     :client  (PaxosClient. nil)
     :checker (checker/linearizable {:model (model/register)})
     :generator (->> (gen/mix [{:f :read} {:f :write :value [(rand-int 10) (rand-int 1000)]}])
                     (gen/stagger 1/10)
                     (gen/limit 50))}))

(defn -main [& args]
  (println "🛡️ Running minimalist linearization check...")
  (let [result (jepsen/run! (paxos-test))]
    (println "Linearizable?" (-> result :results :valid?))))
