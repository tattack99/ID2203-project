(ns paxos-shim.core
  (:require [jepsen.checker :as checker]
            [jepsen.generator :as gen]
            [jepsen.tests :as tests]
            [jepsen.independent :as independent]
            [knossos.model :as model]
            [jepsen.core :as jepsen]
            [jepsen.db :as db]
            [jepsen.os :as os]
            [jepsen.control :as c]
            [jepsen.control.sshj :as sshj]
            [jepsen.nemesis :as nemesis] ;; All partition functions are here
            [paxos-shim.client :as paxos-shim-client]
            [clojure.string :as str]
            [clojure.tools.logging :refer [info]])
  (:gen-class))

;; Global cache for internal Docker IPs (e.g., {"127.0.0.2" "172.18.0.2"})
(def internal-ips (atom {}))

(defn get-ip! [node]
  (let [ip (str/trim (c/exec :hostname :-i))]
    (swap! internal-ips assoc node ip)
    ip))

;; --- IP-AWARE PARTITIONER ---
(defn internal-partitioner
  "Applies the partition using cached 172.x IPs."
  []
  (fn [test op]
    (let [grudge (:value op)]
      (if (or (nil? grudge) (= (:f op) :stop))
        (do (println "🧹 Healing network...")
            (c/with-test-nodes test (c/su (c/exec :iptables-legacy :-F))))
        (do 
          (println "🧱 Partitioning with grudge:" grudge)
          (doseq [[node cut-nodes] grudge]
            (c/on node
              (c/su
                (doseq [cut-node cut-nodes]
                  ;; Use the cached 172.18.x.x IPs
                  (if-let [real-ip (get @internal-ips cut-node)]
                    (do 
                      (println "🧱 [" node "] Blocking" cut-node "at" real-ip)
                      (c/exec :iptables-legacy :-A :INPUT :-s real-ip :-j :DROP))
                    (println "⚠️ [" node "] No cached IP for" cut-node))))))))
      ;; RETURN grudge here, inside the fn but outside the if/else
      grudge)))






(defrecord OmniPaxosDB []
  db/DB
  (setup! [this test node]
    (c/su 
      (c/exec :iptables-legacy :-F)
      (let [my-ip (clojure.string/trim (c/exec :hostname :-i))]
        (println "🚀 Registered" node "as" my-ip)
        (swap! internal-ips assoc node my-ip))))
  (teardown! [this test node]
    (c/su (c/exec :iptables-legacy :-F))))
(defn paxos-test [opts]
  (merge tests/noop-test
    {:name      "paxos-lin-kv"
     :nodes     ["127.0.0.2" "127.0.0.3" "127.0.0.4"]
     :concurrency 9 ; Ensure this is >= the number in concurrent-generator
     :os        (reify os/OS (setup! [_ t n] (c/su (c/exec :iptables :-F))) (teardown! [_ t n] (c/su (c/exec :iptables :-F))))
     :db        (OmniPaxosDB.)
     :client    (paxos-shim-client/paxos-client)
     :checker   (independent/checker (checker/linearizable {:model (model/register)}))
     
     :nemesis (if (= (:mode opts) "partition")
           ;; partitioner returns a Nemesis that handles :start (grudge) and :stop
           (nemesis/partitioner (internal-partitioner))
           nemesis/noop)
      :generator (->> (independent/concurrent-generator
                  3
                  (range)
                  (fn [k]
                    (->> (gen/mix [{:f :read} {:f :write :value (rand-int 100)}])
                         (gen/stagger 1/10)
                         (gen/limit 50))))
                (gen/nemesis
  (cycle [(gen/sleep 5)
          ;; Wrap in (fn [test process]) to ensure (:nodes test) is populated
          (fn [test process]
            {:type :info 
             :f :start 
             :value (nemesis/complete-grudge (nemesis/bisect (:nodes test)))})
          (gen/sleep 15)
          {:type :info :f :stop}]))
                (gen/time-limit 60))}
    opts))

(defn -main [& args]
  (let [mode (or (first args) "check")
        nodes ["127.0.0.2" "127.0.0.3" "127.0.0.4"]]
    (println "🛡️ Starting OmniPaxos Jepsen Test. Mode:" mode)
    (try
      (let [result (jepsen/run! 
                     (assoc (paxos-test {:mode mode :nodes nodes})
                       :remote (sshj/remote)
                       :ssh {:username "root"
                             :password "root"
                             :port 22 ;; Use mapped alias port 22
                             :strict-host-key-checking false}))]
        (println "Test Finished! Linearizable?" (-> result :results :linear :valid?)))
      (catch Exception e
        (println "❌ Jepsen Run Error:" (.getMessage e))
        (.printStackTrace e)))))
