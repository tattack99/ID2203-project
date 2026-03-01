(ns paxos-shim.client
  (:require [clj-http.client :as http]
            [cheshire.core :as json]
            [jepsen.client :as client]))

(defn put! [node-url key val]
  (http/post (str node-url "/put")
             {:content-type :json
              :body (json/generate-string {:key (str key) :value (str val)})}))

(defn get-val [node-url key]
  (let [resp (http/get (str node-url "/get/" key))]
    (:body resp)))

(defrecord PaxosClient [node-url]
  client/Client
  ;; Hardcode port 3001 for the localhost smoke test
  (open! [this test node] 
    (assoc this :node-url "http://localhost:3001"))

  (setup! [this test])

  (invoke! [this test op]
    (try
      (case (:f op)
        :read  (assoc op :type :ok, :value (get-val (:node-url this) "jepsen-key"))
        :write (do (put! (:node-url this) "jepsen-key" (:value op))
                   (assoc op :type :ok)))
      (catch Exception e
        (assoc op :type :info :error (.getMessage e)))))

  (teardown! [this test])
  (close! [this test]))

(defn paxos-client [] (->PaxosClient nil))
