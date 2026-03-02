(ns paxos-shim-kill.ssh-test1
  (:require [jepsen.control :as c]
            [jepsen.control.sshj :as sshj]))

(def ssh-config
  {:username "root"
   :password "root"
   :port 2222
   :strict-host-key-checking false})

(defn test-connection [node]
  (try
    (println "🚀 Testing SSHJ connection to:" node)
    (c/with-remote (sshj/remote) 
      (c/with-ssh ssh-config
        (c/on node
          (let [up (c/exec :uptime)
                ;; This is the critical test for the Nemesis
                ip (c/exec :iptables :-L)]
            (println "✅ Success! " node " uptime says:" up)
            (println "🛡️  Checking iptables rules...")
            (println ip) ; This prints the table if you have permission
            (println "🔥 READY FOR PARTITION TEST!")))))
    (catch Exception e
      (println "❌ Failed to connect to" node ":" (.getMessage e))
      (.printStackTrace e))))


(defn -main [& args]
  (test-connection "localhost"))
