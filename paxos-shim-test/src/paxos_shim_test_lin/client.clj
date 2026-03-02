(ns paxos-shim-test-lin.client
  (:require [clj-http.client :as http]
            [jepsen.client :as client]))

(defrecord PaxosClient [url]
  client/Client
  (open! [this _ node] 
    (assoc this :url (str "http://localhost:" node)))

  (setup! [_ _])

  (invoke! [this _ op]
    (try
      (case (:f op)
        :read  (let [resp (http/get (str (:url this) "/get/jepsen-key"))
                     body (:body resp)]
                 (assoc op :type :ok :value (when-not (or (empty? body) (= body "Key not found")) 
                                              (Integer/parseInt body))))
        :write (do (http/post (str (:url this) "/put")
                              {:form-params {:key "jepsen-key" :value (str (:value op))}
                               :content-type :json})
                   (assoc op :type :ok)))
      (catch Exception e
        (assoc op :type :info :error (.getMessage e)))))

  (teardown! [_ _])
  (close! [_ _]))

(defn paxos-client [] (->PaxosClient nil))
