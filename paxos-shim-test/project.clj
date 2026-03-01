(defproject paxos-shim-test "0.1.0-SNAPSHOT"
  :dependencies [[org.clojure/clojure "1.11.1"]
                 [clj-http "3.12.3"]
                 [cheshire "5.12.0"]]
  :main ^:skip-aot paxos-shim-test.core
  :target-path "target/%s"
  :profiles {:uberjar {:aot :all}})
