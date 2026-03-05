(ns test-nemesis-kill.core
  (:require [jepsen.cli :as cli]
            [jepsen.control :as c]
            [jepsen.db :as db]
            [jepsen.generator :as gen]
            [jepsen.nemesis :as nemesis]
            [jepsen.os :as os]
            [jepsen.tests :as tests]))

(def node-ids {"127.0.0.2" 1 "127.0.0.3" 2 "127.0.0.4" 3})

(defn start! [t node]
  (c/on-nodes t [node]
    (fn [t n]
      (c/su
        (c/exec :sh :-c
          (str "export RUST_BACKTRACE=1 && "
               "export RUST_LOG=debug && "
               "export SERVER_CONFIG_FILE=/app/server-config.toml && "
               "export CLUSTER_CONFIG_FILE=/app/cluster-config.toml && "
               "export OMNIPAXOS_NODE_ADDRS='s1:8000,s2:8000,s3:8000' && "
               "export OMNIPAXOS_LISTEN_ADDRESS=0.0.0.0 && "
               "export OMNIPAXOS_LISTEN_PORT=8000 && "
               "fuser -k 8000/tcp || true && "
               "sleep 1 && "
               "cd /app && "
               "setsid /usr/local/bin/server > /app/logs/nemesis.log 2>&1 &")))))
  :started)

(defn stop! [t node]
  (c/on-nodes t [node]
    (fn [t n]
      (try
        (c/su (c/exec :pkill :-9 :-f "server"))
        (catch Exception _ :already-stopped))))
  :stopped)

(defn nemesis-test [opts]
  (merge tests/noop-test opts
         {:name    "kill-test"
          :nodes   ["127.0.0.2" "127.0.0.3" "127.0.0.4"]
          :os      os/noop
          :db      db/noop
          :nemesis (nemesis/node-start-stopper (fn [t nodes] (rand-nth nodes)) stop! start!)
          :generator (->> (gen/nemesis (gen/cycle [(gen/sleep 10) {:type :info :f :stop}
                                                   (gen/sleep 10) {:type :info :f :start}]))
                          (gen/time-limit 40))}))

(defn -main [& args] (cli/run! (cli/single-test-cmd {:test-fn nemesis-test}) args))
