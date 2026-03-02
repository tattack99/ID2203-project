(defproject paxos "0.1.0-SNAPSHOT"
  :dependencies [[org.clojure/clojure "1.12.4"]
                 [clj-http "3.13.1"]
                 [cheshire "5.13.0"]
                 [jepsen "0.3.10" :exclusions [org.slf4j/slf4j-log4j12 log4j/log4j]] 
                 [knossos "0.3.13"]]
  :main ^:skip-aot paxos.core)
