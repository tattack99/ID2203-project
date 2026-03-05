(ns nemesis-partition.core
  (:require [clojure.java.shell :refer [sh]]
            [clojure.string :as str]
            [clj-http.client :as http]
            [jepsen [cli :as cli]
                    [checker :as checker]
                    [control :as c]
                    [db :as db]
                    [generator :as gen]
                    [nemesis :as nemesis]
                    [tests :as tests]
                    [client :as client]]
            [knossos.model :as model])
  (:gen-class))

;; SSH IPs → HTTP endpoints for the Jepsen client
(def nodes
  {"127.0.0.2" "http://localhost:3001"
   "127.0.0.3" "http://localhost:3002"
   "127.0.0.4" "http://localhost:3003"})

(def http-opts {:conn-timeout 1000 :socket-timeout 1000})

;; --- Partition helpers ---
;; The servers communicate via Docker-internal IPs (172.18.x.x), not the
;; SSH loopback IPs (127.0.0.x). iptables rules must use the real Docker IPs.

(defn docker-ip [container]
  (str/trim (:out (sh "docker" "inspect" "-f"
                      "{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}"
                      container))))

(defn ssh->docker-ip []
  ;; Build a map from SSH IP -> Docker internal IP
  (zipmap (keys nodes)
          (map docker-ip ["s1" "s2" "s3"])))

(defn partition-grudge
  "Returns a grudge map suitable for nemesis/partitioner.
   Keys are SSH IPs (for Jepsen to SSH into), values are sets of Docker IPs
   to block via iptables."
  [ssh-nodes]
  (let [ip-map (ssh->docker-ip)
        halves (nemesis/bisect (vec ssh-nodes))
        grudge (nemesis/complete-grudge halves)]
    ;; Translate grudge targets from SSH IPs to Docker internal IPs
    (into {} (for [[src blocked] grudge]
               [src (set (map ip-map blocked))]))))

;; --- Jepsen client: read/write via HTTP ---

(defrecord PaxosClient [url]
  client/Client
  (open!    [this _ node] (assoc this :url (get nodes node)))
  (setup!   [_ _])
  (teardown![_ _])
  (close!   [_ _])
  (invoke!  [this _ op]
    (try
      (case (:f op)
        :read  (let [body (:body (http/get (str url "/get/jepsen-key") http-opts))]
                 (assoc op :type :ok
                           :value (when (not (or (empty? body) (= body "Key not found")))
                                    (Integer/parseInt body))))
        :write (do (http/post (str url "/put")
                              (merge http-opts
                                     {:form-params {:key "jepsen-key" :value (str (:value op))}
                                      :content-type :json}))
                   (assoc op :type :ok)))
      (catch Exception e
        (assoc op :type :info :error (.getMessage e))))))

;; --- Test definition ---

(defn omnipaxos-test [opts]
  (merge tests/noop-test opts
    {:name    "omnipaxos-partition"
     :nodes   (keys nodes)
     :db      db/noop
     :client  (->PaxosClient nil)
     :nemesis (nemesis/partitioner partition-grudge)
     :checker (checker/compose
                {:linear (checker/linearizable {:model (model/register)})})
     :generator
     (->> (gen/mix [(fn [_ _] {:f :read})
                    (fn [_ _] {:f :write :value (rand-int 100)})])
          (gen/stagger 1/5)
          (gen/nemesis
            (gen/cycle [(gen/sleep 10)             ; let cluster stabilize
                        {:type :info :f :start}    ; apply network partition
                        (gen/sleep 20)             ; keep partitioned (minority loses quorum)
                        {:type :info :f :stop}     ; heal partition
                        (gen/sleep 10)]))           ; wait for reconnect + leader stabilize
          (gen/time-limit (:time-limit opts)))}))

(defn -main [& args]
  (cli/run! (cli/single-test-cmd {:test-fn omnipaxos-test}) args))
