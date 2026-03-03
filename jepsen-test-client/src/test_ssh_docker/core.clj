(ns test-ssh-docker.core
  (:require [clojure.java.shell :refer [sh]]
            [clojure.string :as str]
            [jepsen.control :as c]
            [jepsen.control.sshj :as sshj])
  (:import [org.slf4j LoggerFactory]
           [ch.qos.logback.classic Level Logger]))

;; Silence the noisy SSHJ/Apache debug logs
(defn- silence-logging! []
  (.setLevel ^Logger (LoggerFactory/getLogger "net.schmizz.sshj") Level/WARN)
  (.setLevel ^Logger (LoggerFactory/getLogger "org.apache.http") Level/WARN))

(def ssh-config
  {:username "root" :password "root" :strict-host-key-checking false})

(defn get-ip [container]
  (let [res (sh "docker" "inspect" "-f" "{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}" container)]
    (str/trim (:out res))))

(defn test-node [id ip]
  (let [host (str "127.0.0." (+ id 1))]
    (try
      (c/with-ssh ssh-config
        (c/with-remote (sshj/remote)
          (c/on host
            (let [rules (c/exec :iptables :-L :INPUT :-n)]
              (println (str "PROBE: s" id " | " host " | Internal: " ip " | SSH: OK"))
              (println (str "PROBE: s" id " Firewalls:\n" rules))))))
      (catch Exception e
        (println (str "PROBE: s" id " | " host " | SSH: FAILED - " (.getMessage e)))))))

(defn -main [& _]
  (silence-logging!)
  (println "PROBE: Starting cluster discovery...")
  (try
    (let [servers ["s1" "s2" "s3"]
          ips (mapv get-ip servers)]
      (dotimes [i (count ips)]
        (test-node (inc i) (nth ips i))))
    (catch Exception e
      (println "PROBE: Setup Error:" (.getMessage e)))))
