(ns nemesis-recovery.core
  (:gen-class)
  (:require [clojure.tools.logging :refer [info]]
            [clj-http.client :as http]
            [jepsen [cli :as cli]
                    [checker :as checker]
                    [control :as c]
                    [db :as db]
                    [generator :as gen]
                    [nemesis :as nemesis]
                    [tests :as tests]
                    [client :as client]]
            [knossos.model :as model]))

;; SSH IPs → HTTP endpoints for the Jepsen client
(def nodes
  {"127.0.0.2" "http://localhost:3001"
   "127.0.0.3" "http://localhost:3002"
   "127.0.0.4" "http://localhost:3003"})

(def http-opts {:conn-timeout 500 :socket-timeout 500})

;; --- Server control ---

(defn stop-server! [test node]
  (info node "killing server")
  (c/on-nodes test [node]
    (fn [_ _] (c/su (try (c/exec :pkill :-9 :-f "server")
                         (catch Exception _ nil)))))
  :killed)

(defn start-server! [test node]
  (info node "starting server")
  (c/on-nodes test [node]
    (fn [_ _]
      (c/su
        (c/exec :sh :-c
          (str "export RUST_LOG=info"
               " RUST_BACKTRACE=1"
               " SERVER_CONFIG_FILE=/app/server-config.toml"
               " CLUSTER_CONFIG_FILE=/app/cluster-config.toml"
               " OMNIPAXOS_NODE_ADDRS='s1:8000,s2:8000,s3:8000'"
               " OMNIPAXOS_LISTEN_ADDRESS=0.0.0.0"
               " OMNIPAXOS_LISTEN_PORT=8000 && "
               "fuser -k 8000/tcp || true && sleep 1 && "
               "cd /app && setsid /usr/local/bin/server > /app/logs/recovery.log 2>&1 &")))))
  :started)

;; --- Custom nemesis: crash ALL nodes simultaneously ---
;;
;; This is the core test for file-based persistence. With a full cluster crash,
;; the ONLY way committed data survives is from persistent storage.
;; Single-node crashes (nemesis-kill) don't prove this — the 2 surviving nodes
;; still hold the data in memory. Here, nothing is in memory after the crash.
;; Each node persists committed KV state to /app/logs/db_<id>.json (Docker volume).

(defn crash-recovery-nemesis []
  (reify nemesis/Nemesis
    (setup! [this _test] this)
    (invoke! [this test op]
      (case (:f op)
        :crash-all
        (do
          (info "Crashing ALL nodes — full cluster crash to test file-based persistence")
          (doseq [node (:nodes test)]
            (stop-server! test node))
          (assoc op :type :info :value :all-crashed))
        :restart-all
        (do
          (info "Restarting ALL nodes — committed data must be restored from file DB snapshot")
          (doseq [node (:nodes test)]
            (start-server! test node))
          (assoc op :type :info :value :all-restarted))))
    (teardown! [this _test] this)))

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
;;
;; Generator cycles:
;;   1. Write data for 20s (commits land in /app/logs/db_<id>.json snapshot)
;;   2. Crash ALL nodes simultaneously (no in-memory state survives)
;;   3. Pause 5s for processes to fully die
;;   4. Restart all nodes (each loads DB snapshot, MemoryStorage starts fresh at idx=0)
;;   5. Wait 45s for leader election + cluster stabilization before next ops
;;
;; Knossos verifies that every read after recovery sees a value consistent
;; with the write history before the crash — proving no committed data was lost.

(defn omnipaxos-test [opts]
  (merge tests/noop-test opts
    {:name    "omnipaxos-recovery"
     :nodes   (keys nodes)
     :db      db/noop
     :client  (->PaxosClient nil)
     :nemesis (crash-recovery-nemesis)
     :checker (checker/compose
                {:linear (checker/linearizable {:model (model/register)})})
     :generator
     (->> (gen/mix [(fn [_ _] {:f :read})
                    (fn [_ _] {:f :write :value (rand-int 100)})])
          (gen/stagger 1/5)
          (gen/nemesis
            (gen/cycle
              [(gen/sleep 20)               ; let the cluster commit some data
               {:type :info :f :crash-all}  ; kill ALL 3 nodes at once
               (gen/sleep 5)               ; wait for processes to fully die
               {:type :info :f :restart-all} ; restart — each node loads file DB snapshot
               (gen/sleep 45)]))           ; wait for leader election + DB reconstruction
          (gen/time-limit (:time-limit opts)))}))

(defn -main [& args]
  (cli/run! (cli/single-test-cmd {:test-fn omnipaxos-test}) args))
