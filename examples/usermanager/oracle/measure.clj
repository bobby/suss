;; Development-only comparison: same app, persistent-map capability adapter.
(def source-start (System/nanoTime))
(require '[clojure.data.json :as json] '[usermanager.storage :as storage] '[usermanager.app :as app])
(def callback-totals (atom {:ns 0 :calls 0}))
(doseq [name '[key open close! users departments get-user put-department insert-user update-user delete-user]]
  (alter-var-root (ns-resolve 'usermanager.storage name)
    (fn [original]
      (fn [& args]
        (let [started (System/nanoTime)]
          (try (apply original args)
            (finally (let [elapsed (- (System/nanoTime) started)]
                       (swap! callback-totals #(-> % (update :ns + elapsed) (update :calls inc)))))))))))
(defn mark [] [(System/nanoTime) @callback-totals])
(defn emit [before repetition operation count]
  (let [elapsed (- (System/nanoTime) (first before)) totals @callback-totals]
    (println (json/write-str {:phase "warm" :repetition repetition :operation operation :count count :ns elapsed
      :host-capability-ns (- (:ns totals) (:ns (second before)))
      :host-capability-calls (- (:calls totals) (:calls (second before)))}))))
(defn temporary-form [id label]
  {:id (if id [:some (str id)] [:none]) :first-name [:some [:some label]]
   :last-name [:none] :email [:none] :department-id [:some "4"]})
(let [size (Long/parseLong (first *command-line-args*))
      db (app/unwrap (app/open ":memory:"))
      first-response (app/unwrap (app/request db [:departments]))]
  (assert (empty? (:departments first-response)))
  (println (json/write-str {:phase "first-response" :clojure (clojure-version) :dataset size
    :source-load-through-first-response-ns (- (System/nanoTime) source-start)}))
  (flush)
  (app/unwrap (app/request db [:seed-department {:id 4 :name "Development"}]))
  (let [before (mark)]
    (dotimes [id size]
      (app/unwrap (app/request db [:save-user {:id [:none]
        :first-name [:some [:some (format "Name %08d λ🦀" id)]]
        :last-name [:some [:some (format "Family %08d" (- size id))]]
        :email [:some [:some (str "user" id "@example.test")]] :department-id [:some "4"]}])))
    (emit before 0 "setup-through-guest-crud" size))
  (dotimes [repetition 5]
    (let [changes (+ size (* 300 repetition)) before (mark)
          view (app/unwrap (app/request db [:list-users]))]
      (assert (= size (count (:users view))))
      (assert (= changes (:changes view)))
      (assert (= "Family 00000001" (get-in view [:users 0 :value :last-name 1])))
      (emit before repetition "joined-sorted-list" size)
      (let [before (mark)]
        (dotimes [_ 1000] (assert (= changes (app/changes db))))
        (emit before repetition "guest-host-key-change-counter" 1000))
      (let [before (mark)]
        (dotimes [index 100]
          (let [id (+ size (* repetition 100) index 1)]
            (app/unwrap (app/request db [:save-user (temporary-form nil "temporary")]))
            (let [edit (app/unwrap (app/request db [:edit-user [:some id]]))]
              (assert (= [:some "temporary"] (get-in edit [:user 1 :first-name]))))
            (app/unwrap (app/request db [:save-user (temporary-form id "updated")]))
            (app/unwrap (app/request db [:delete-user id]))))
        (emit before repetition "create-read-update-delete" 100))
      (assert (= (+ changes 300) (app/changes db)))))
  (app/close db)
  (assert (empty? (:sessions @storage/state)))
  (println (json/write-str {:phase "closed" :dataset size :opened (:opened @storage/state) :closed (:closed @storage/state)
    :host-trace-count (count (:trace @storage/state)) :host-capability @callback-totals
    :heap-usage (str (.getHeapMemoryUsage (java.lang.management.ManagementFactory/getMemoryMXBean)))
    :non-heap-usage (str (.getNonHeapMemoryUsage (java.lang.management.ManagementFactory/getMemoryMXBean)))
    :limitations "JVM persistent-map adapter uses timed Clojure function calls, not canonical WIT. Includes instrumentation overhead; no subtraction. Java is a development oracle only."})))
