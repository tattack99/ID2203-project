(ns paxos-shim.partition-check
  (:require [jepsen.control :as c]
            [jepsen.control.sshj :as sshj]
            [clojure.string :as str]))

(def ssh-defaults
  {:username "root"
   :password "root"
   :port     22
   :strict-host-key-checking false})

(defn -main [& args]
  (try
    ;; 1. Initialize the SSH driver
    (c/with-remote (sshj/remote)
      (println "🔍 Resolving internal IPs via Loopback Aliases...")
      
      (let [s1-alias "127.0.0.2"
            s2-alias "127.0.0.3"]
        
        ;; 2. Get s2's internal IP
        (let [s2-internal-ip (c/with-ssh (assoc ssh-defaults :host s2-alias)
                               (c/on s2-alias 
                                 (str/trim (c/exec :hostname :-i))))]
          (println "📍 s2 Internal IP (172.x):" s2-internal-ip)

          ;; 3. Connect to s1 to apply the rule
          (c/with-ssh (assoc ssh-defaults :host s1-alias)
            (c/on s1-alias
              (println "🌐 [s1] Connected. Blocking s2 traffic...")
              
              ;; Use su to ensure we have root permissions for iptables
              (c/su
                (c/exec :iptables :-A :INPUT :-s s2-internal-ip :-j :DROP)
                
                (let [rules (c/exec :iptables :-L :-n)]
                  (if (str/includes? rules s2-internal-ip)
                    (println "✅ SUCCESS: Partition active! s1 is dropping s2's packets.")
                    (println "❌ FAILURE: Rule missing from iptables.")))

                ;; Cleanup immediately so we don't lock ourselves out
                (println "🧹 Cleaning up rules on s1...")
                (c/exec :iptables :-F)))))))
                
    (catch Exception e 
      (println "💥 Error:" (.getMessage e))
      (if-let [data (ex-data e)]
        (prn data)
        (.printStackTrace e)))))
