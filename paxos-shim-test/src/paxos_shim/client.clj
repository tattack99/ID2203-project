(ns paxos-shim.client
  (:require [clj-http.client :as http]
            [cheshire.core :as json]
            [jepsen.client :as client]
            [jepsen.independent :as independent]))

(defn put! [node-url key val]
  (http/post (str node-url "/put")
             {:content-type :json
              :body (json/generate-string {:key (str key) :value (str val)})}))

(defn get-val [node-url key]
  (let [resp (http/get (str node-url "/get/" key) {:throw-exceptions false})]
    (let [body (:body resp)]
      (if (or (nil? body) (= "" body) (= "Key not found" body))
        nil
        (Integer/parseInt body)))))

(defrecord PaxosClient [node-url]
  client/Client
  (open! [this test node]
    (let [port (get {"127.0.0.2" 3001 
                     "127.0.0.3" 3002 
                     "127.0.0.4" 3002} node)] ;; Point s3 requests to c2
      (assoc this :node-url (str "http://localhost:" port))))

  (setup! [this test])

  (invoke! [this test op]
    (let [[k v] (:value op)] 
      (try
        (case (:f op)
          :read  (let [res (get-val (:node-url this) k)]
                   (assoc op :type :ok, :value (independent/tuple k res)))
          :write (do (put! (:node-url this) k v)
                     (assoc op :type :ok)))
        (catch Exception e
          (assoc op :type :info :error (.getMessage e))))))

  (teardown! [this test])
  (close! [this test])) ;; <--- This was the missing parenthesis

(defn paxos-client [] (->PaxosClient nil))
