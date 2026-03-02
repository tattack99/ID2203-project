(ns nemesis-partition.core
  (:require [clojure.java.shell :refer [sh]]
            [clojure.string :as str]
            [clj-http.client :as http]
            [jepsen.checker :as checker]
            [jepsen.generator :as gen]
            [jepsen.tests :as tests]
            [jepsen.nemesis :as nemesis]
            [jepsen.control.sshj :as sshj]
            [jepsen.client :as client]
            [jepsen.core :as jepsen]
            [knossos.model :as model])
  (:import [org.slf4j LoggerFactory]
           [ch.qos.logback.classic Level Logger])
  (:gen-class))

(defn- setup-logging! []
  (.setLevel ^Logger (LoggerFactory/getLogger "jepsen.nemesis") Level/INFO)
  (.setLevel ^Logger (LoggerFactory/getLogger "jepsen.core") Level/INFO)
  (.setLevel ^Logger (LoggerFactory/getLogger "net.schmizz.sshj") Level/WARN))

(defn get-ip [container]
  (let [res (sh "docker" "inspect" "-f" "{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}" container)]
    (str/trim (:out res))))

(defrecord PaxosClient [node-map]
  client/Client
  (open! [this _ node] (assoc this :url (get node-map node)))
  (setup! [_ _])
  (invoke! [this _ op]
    (try
      (case (:f op)
        :read  (let [resp (http/get (str (:url this) "/get/jepsen-key") 
                                   {:conn-timeout 1000 :socket-timeout 1000}) ; <--- ADD THIS
                     b (:body resp)]
                 (assoc op :type :ok :value (when-not (or (empty? b) (= b "Key not found")) 
                                              (Integer/parseInt b))))
        :write (do (http/post (str (:url this) "/put")
                              {:form-params {:key "jepsen-key" :value (str (:value op))}
                               :content-type :json 
                               :conn-timeout 1000 
                               :socket-timeout 1000}) ; <--- ADD THIS
                   (assoc op :type :ok)))
      (catch Exception e (assoc op :type :info :error (.getMessage e))))) ; Records the failure and moves on
  (teardown! [_ _])
  (close! [_ _]))


(defn paxos-test []
  (let [ssh-hosts    ["127.0.0.2" "127.0.0.3" "127.0.0.4"]
        ;; Hardcoded mapping: SSH Alias -> Internal Docker IP
        internal-map {"127.0.0.2" "172.18.0.6" 
                      "127.0.0.3" "172.18.0.3" 
                      "127.0.0.4" "172.18.0.4"}
        node-map     {"127.0.0.2" "http://localhost:3001" 
                      "127.0.0.3" "http://localhost:3002" 
                      "127.0.0.4" "http://localhost:3001"}] ; Note: c1/c2 mapping
    (merge tests/noop-test
           {:name      "paxos-partition"
            :remote    (sshj/remote)
            :ssh       {:username "root" :password "root" :strict-host-key-checking false}
            :nodes     ssh-hosts
            :nemesis   (nemesis/partitioner 
                         (fn [nodes]
                           (let [halves (nemesis/bisect nodes)
                                 grudge (nemesis/complete-grudge halves)]
                             ;; Key = SSH IP (for Jepsen), Value = Docker IP (for iptables)
                             (into {} (for [[src dests] grudge]
                                        [src (set (map internal-map dests))])))))
            :client    (->PaxosClient node-map)
            :checker   (checker/compose {:linear (checker/linearizable {:model (model/register)})})
            :generator (->> (gen/mix [(fn [_ _] {:f :read}) 
                                     (fn [_ _] {:f :write :value (rand-int 100)})])
                            (gen/stagger 1/5)
                            (gen/nemesis (gen/cycle [(gen/sleep 5) 
                                                   {:type :info :f :start} 
                                                   (gen/sleep 5) 
                                                   {:type :info :f :stop}]))
                            (gen/time-limit 30))})))



(defn -main [& _]
  (setup-logging!)
  (println "🛡️ Starting Partition Test...")
  (try
    (let [result (jepsen/run! (paxos-test))]
      (println "\nOVERALL RESULT:" (if (-> result :results :linear :valid?) "✅ PASS" "❌ FAIL")))
    (catch Exception e (println "💥 Error:" (.getMessage e)))))
