(ns paxos-shim-test.core
  (:require [clj-http.client :as http]
            [cheshire.core :as json])
  (:gen-class))

;; Define our Client target URLs
(def nodes ["http://localhost:3001" "http://localhost:3002"])

(defn paxos-put! [node key val]
  (try
    (let [url (str node "/put")
          body (json/generate-string {:key (str key) :value (str val)})]
      (http/post url {:content-type :json :body body})
      :ok)
    (catch Exception e (println "Put Error:" (.getMessage e)) :fail)))

(defn paxos-get [node key]
  (try
    (let [url (str node "/get/" key)
          resp (http/get url)]
      (:body resp))
    (catch Exception e (println "Get Error:" (.getMessage e)) nil)))

(defn -main [& args]
  (let [k "my-paxos-key"
        v (str (rand-int 10000))]
    (println "🚀 Testing Paxos Shim via Clojure...")
    
    ;; Write to C1
    (println "Writing" v "to Node 0...")
    (if (= :ok (paxos-put! (first nodes) k v))
      (do
        ;; Read from C2
        (println "Reading from Node 1...")
        (let [result (paxos-get (second nodes) k)]
          (println "Result:" result)
          (if (= v result)
            (println "🎊 SUCCESS: Clojure confirmed replication!")
            (println "❌ FAILURE: Data mismatch."))))
      (println "❌ FAILURE: Put request failed."))))
