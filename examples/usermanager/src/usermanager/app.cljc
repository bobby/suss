;; Adapted from UserManager, copyright (c) 2019-2023 Sean Corfield.
;; Licensed under Apache-2.0; see ../../upstream/LICENSE and provenance.json.
(ns usermanager.app)

(defprotocol Lifecycle (start [this]) (stop [this]))
(defrecord Database [db-spec datasource]
  Lifecycle
  (start [this] (if datasource this (assoc this :datasource db-spec)))
  (stop [this] (assoc this :datasource nil))
  #?(:clj clojure.lang.IFn :default IFn)
  #?(:clj (invoke [_] datasource) :default (-invoke [_] datasource)))

(defn unwrap [result]
  (if (= (nth result 0) :ok) (nth result 1 nil)
      (throw (ex-info (:message (nth result 1)) (nth result 1)))))
(defn option-value [value] (if (= (nth value 0) :some) (nth value 1) nil))
(defn option [value] (if (nil? value) [:none] [:some value]))
(defn row-in [row]
  {:addressbook/id (:id row)
   :addressbook/first_name (option-value (:first-name row))
   :addressbook/last_name (option-value (:last-name row))
   :addressbook/email (option-value (:email row))
   :addressbook/department_id (option-value (:department-id row))})
(defn row-out [row]
  {:id (:addressbook/id row)
   :first-name (option (:addressbook/first_name row))
   :last-name (option (:addressbook/last_name row))
   :email (option (:addressbook/email row))
   :department-id (option (:addressbook/department_id row))})
(defn patch-field [row key] (if (contains? row key) [:some (option (get row key))] [:none]))
(defn patch-out [row]
  {:first-name (patch-field row :addressbook/first_name)
   :last-name (patch-field row :addressbook/last_name)
   :email (patch-field row :addressbook/email)
   :department-id (option (:addressbook/department_id row))})

(defn get-departments [db]
  (vec (sort (fn [a b] (compare (:name a) (:name b)))
             (unwrap (usermanager.storage/departments (db))))))
(defn get-user-by-id [db id]
  (let [row (option-value (unwrap (usermanager.storage/get-user (db) id)))]
    (when row (row-in row))))
(defn get-users [db]
  (let [departments (into {} (map (fn [row] [(:id row) (:name row)]) (get-departments db)))
        users (mapv row-in (unwrap (usermanager.storage/users (db))))]
    (vec (sort (fn [a b] (compare [(:addressbook/last_name a) (:addressbook/first_name a)]
                                  [(:addressbook/last_name b) (:addressbook/first_name b)]))
               (map (fn [row] (assoc row :department/name (get departments (:addressbook/department_id row))))
                    (filter (fn [row] (contains? departments (:addressbook/department_id row))) users))))))
(defn save-user [db user]
  (let [id (:addressbook/id user)
        values (patch-out (dissoc user :addressbook/id))]
    (if (and id (not (zero? id)))
      (unwrap (usermanager.storage/update-user (db) id values))
      (unwrap (usermanager.storage/insert-user (db) values)))))
(defn delete-user-by-id [db id] (unwrap (usermanager.storage/delete-user (db) id)))

(defn form-long [text]
  (if (or (nil? text) (= text "")) nil
      (let [value (parse-long text)]
        (if (nil? value) (throw (ex-info "Invalid form ID" {:kind :parse :message "Invalid form ID"})) value))))
(defn form-in [form]
  (reduce-kv (fn [row wire-key value]
               (let [key (get {:first-name :first_name :last-name :last_name :email :email} wire-key)]
                 (if (= (nth value 0) :some) (assoc row key (option-value (nth value 1))) row)))
             {:id (option-value (:id form)) :department_id (option-value (:department-id form))}
             (select-keys form [:first-name :last-name :email])))
(defn redirect [_] {:location "/user/list" :application/view :redirect})
(defn save [req]
  (swap! (get-in req [:application/component :changes]) inc)
  (-> req :params
      (select-keys [:id :first_name :last_name :email :department_id])
      (update :id form-long)
      (update :department_id form-long)
      (->> (reduce-kv (fn [m k v] (assoc! m (keyword "addressbook" (name k)) v)) (transient {}))
           (persistent!)
           (save-user (get-in req [:application/component :database]))))
  (redirect req))
(defn delete-by-id [req]
  (swap! (get-in req [:application/component :changes]) inc)
  (delete-user-by-id (get-in req [:application/component :database]) (get-in req [:params :id]))
  (redirect req))
(defn edit [req]
  (let [db (get-in req [:application/component :database])
        user (when-let [id (get-in req [:params :id])] (get-user-by-id db id))]
    (-> req (update :params assoc :user user :departments (get-departments db))
        (assoc :application/view :form))))
(defn list-users [req]
  (-> req (assoc-in [:params :users] (get-users (get-in req [:application/component :database])))
      (assoc :application/view :list)))
(defn reset-changes [req]
  (reset! (get-in req [:application/component :changes]) 0)
  (-> req (assoc-in [:params :message] "The change tracker has been reset.")
      (assoc :application/view :message)))

(def sessions (atom {}))
(defn open [path]
  (let [result (usermanager.storage/open path)]
    (if (= (nth result 0) :ok)
      (let [handle (nth result 1)]
        (swap! sessions assoc (usermanager.storage/key handle) (atom 0))
        [:ok handle]) result)))
(defn changes [handle] @(get @sessions (usermanager.storage/key handle)))
(defn close [handle]
  (let [key (usermanager.storage/key handle)]
    (swap! sessions dissoc key)
    #?(:clj (usermanager.storage/close! handle) :default (suss.resource/close! handle))
    key))
(defn render-page [req count]
  (let [params (:params req)]
    {:kind (:application/view req)
     :users (mapv (fn [row] {:value (row-out row) :department-name (:department/name row)}) (:users params []))
     :user (if (:user params) [:some (row-out (:user params))] [:none])
     :departments (:departments params []) :changes count
     :location (option (:location req)) :message (option (:message params))}))
(defn request [handle value]
  (let [counter (get @sessions (usermanager.storage/key handle))
        db (start (map->Database {:db-spec handle}))
        base {:application/component {:database db :changes counter} :params {}}
        tag (nth value 0)]
    (try
      (let [req
            (case tag
              :list-users (list-users base)
              :edit-user (edit (assoc base :params {:id (option-value (nth value 1))}))
              :save-user (save (assoc base :params (form-in (nth value 1))))
              :delete-user (delete-by-id (assoc base :params {:id (nth value 1)}))
              :departments (-> base (assoc-in [:params :departments] (get-departments db))
                                (assoc :application/view :departments))
              :reset-changes (reset-changes base)
              :seed-department (do (unwrap (usermanager.storage/put-department (db) (nth value 1)))
                                   (assoc base :application/view :departments)))]
        (stop db)
        [:ok (render-page req @counter)])
      (catch #?(:clj Exception :default :default) error
        (let [data (ex-data error)]
          (if (contains? #{:parse :storage :invalid} (:kind data))
            [:err {:kind (:kind data) :message (ex-message error)}]
            (throw error)))))))
