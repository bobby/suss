;; Development-only deterministic persistence capability for the shared JVM app.
(ns usermanager.storage (:refer-clojure :exclude [key]))
(def state (atom {:opened 0 :closed 0 :trace [] :sessions {}}))
(def initial (atom {:next-id 0 :users {} :departments {}}))
(defn before [operation]
  (swap! state update :trace conj operation)
  (when-let [message (:fail-next @state)]
    (swap! state dissoc :fail-next)
    [:err {:kind :storage :message message}]))
(defn key [db] db)
(defn open [_]
  (or (before "open")
      (let [id (:opened (swap! state update :opened inc))]
        (swap! state assoc-in [:sessions id] @initial)
        [:ok id])))
(defn close! [db]
  (swap! state (fn [s] (-> s (update :closed inc) (update :sessions dissoc db)
                           (update :trace conj (str "close:" db)))))
  nil)
(defn users [db] (or (before "users") [:ok (vec (vals (get-in @state [:sessions db :users])))]))
(defn departments [db] (or (before "departments") [:ok (vec (vals (get-in @state [:sessions db :departments])))]))
(defn get-user [db id]
  (or (before "get-user") (if-let [row (get-in @state [:sessions db :users id])] [:ok [:some row]] [:ok [:none]])))
(defn put-department [db value]
  (or (before "put-department")
      (do (swap! state assoc-in [:sessions db :departments (:id value)] value) [:ok nil])))
(defn apply-patch [row patch]
  (reduce-kv (fn [row key value]
               (if (= key :department-id) (assoc row key value)
                   (if (= (first value) :some) (assoc row key (second value)) row))) row patch))
(defn insert-user [db patch]
  (or (before "insert-user")
      (when (= (:department-id patch) [:none]) [:err {:kind :storage :message "department_id must not be null"}])
      (let [id (inc (get-in @state [:sessions db :next-id]))
            row (apply-patch {:id id :first-name [:none] :last-name [:none] :email [:none] :department-id [:none]} patch)]
        (swap! state (fn [s] (-> s (assoc-in [:sessions db :next-id] id) (assoc-in [:sessions db :users id] row))))
        [:ok row])))
(defn update-user [db id patch]
  (or (before "update-user")
      (if-let [row (get-in @state [:sessions db :users id])]
        (if (= (:department-id patch) [:none])
          [:err {:kind :storage :message "department_id must not be null"}]
          (let [row (apply-patch row patch)]
            (swap! state assoc-in [:sessions db :users id] row) [:ok [:some row]]))
        [:ok [:none]])))
(defn delete-user [db id]
  (or (before "delete-user")
      (let [exists (contains? (get-in @state [:sessions db :users]) id)]
        (swap! state update-in [:sessions db :users] dissoc id) [:ok exists])))
