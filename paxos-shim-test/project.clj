(defproject paxos-shim "0.1.0-SNAPSHOT"
  :dependencies [[org.clojure/clojure "1.12.4"]           ;; Latest Clojure
                 [clj-http "3.13.1"]                       ;; Latest clj-http
                 [cheshire "5.13.0"]                       ;; Latest Cheshire
                 [jepsen "0.3.10" :exclusions [org.slf4j/slf4j-log4j12 log4j/log4j]] 
                 [knossos "0.3.13"]]                       ;; Match Jepsen 0.3.10
  :main ^:skip-aot paxos-shim.core)
