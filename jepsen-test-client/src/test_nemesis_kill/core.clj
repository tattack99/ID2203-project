(ns test-nemesis-kill.core
  (:gen-class)
  (:require [clojure.tools.logging :refer [info]]
            [jepsen [cli :as cli]
                    [control :as c]
                    [db :as db]
                    [generator :as gen]
                    [nemesis :as nemesis]
                    [tests :as tests]]))

(defn start-server! [test node]
  (info "Nemesis: Starting server on" node)
  (c/exec :touch :_jepsen_was_here)
  :started)

(defn stop-server! [test node]
  (info "Nemesis: Killing server on" node)
  (c/exec :sh :-c "pkill -9 server || true")
  :killed)

(defn nemesis-test
  [opts]
  (merge tests/noop-test
         opts
         {:name    "nemesis-kill-verify"
          :db      db/noop
          :nemesis (nemesis/node-start-stopper 
                     (fn [test nodes] (rand-nth nodes)) 
                     start-server! 
                     stop-server!)
          :generator (->> (gen/nemesis
                            (gen/cycle [ (gen/sleep 5)
                                         {:type :info, :f :stop}
                                         (gen/sleep 5)
                                         {:type :info, :f :start} ]))
                          (gen/time-limit 30))}))

(defn -main
  [& args]
  (cli/run! (cli/single-test-cmd {:test-fn nemesis-test})
            (concat ["test"
                     "--nodes" "127.0.0.2,127.0.0.3,127.0.0.4"
                     "--username" "root"
                     "--password" "root"
                     "--time-limit" "30"
                     "--concurrency" "1"]
                    args)))
