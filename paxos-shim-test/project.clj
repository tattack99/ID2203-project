(defproject paxos-shim "0.1.0-SNAPSHOT"
  :dependencies [[org.clojure/clojure "1.11.1"]
                 [clj-http "3.12.3"]
                 [cheshire "5.12.0"]
                 [jepsen "0.3.6" :exclusions [org.slf4j/slf4j-log4j12 log4j/log4j]]
                 [knossos "0.3.10"]] ;; Add this for models
  :main ^:skip-aot paxos-shim.core)
