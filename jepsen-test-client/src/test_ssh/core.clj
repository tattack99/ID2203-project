(ns test-ssh.core
  (:require [jepsen.control :as c]
            [jepsen.control.sshj :as sshj]))

(def ssh-config
  {:username "root"
   :password "root"
   :strict-host-key-checking false})

(defn test-connection [node]
  (println "🚀 Testing SSHJ to:" node)
  (try
    (c/with-ssh ssh-config
      (c/with-remote (sshj/remote)
        (c/on node
          (println "✅ Uptime on" node ":" (c/exec :uptime)))))
    (catch Exception e
      (println "❌ Failed on" node ":" (.getMessage e)))))

(defn -main [& _]
  (doseq [node ["127.0.0.2" "127.0.0.3" "127.0.0.4"]]
    (test-connection node)))
