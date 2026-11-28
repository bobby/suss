(require '[clojure.data.json :as json] '[clojure.string :as str]
         '[usermanager.storage :as storage] '[usermanager.app :as app])
(defn option [value] (if (nil? value) [:none] [:some value]))
(defn field [form key] (if (contains? form key) [:some (option (get form key))] [:none]))
(defn request [input]
  (case (:op input)
    "list-users" [:list-users]
    "departments" [:departments]
    "reset-changes" [:reset-changes]
    "seed-department" [:seed-department (select-keys input [:id :name])]
    "edit-user" [:edit-user (option (:id input))]
    "delete-user" [:delete-user (:id input)]
    "save-user" (let [form (:form input)]
                  [:save-user {:id (option (:id form)) :department-id (option (:department_id form))
                               :first-name (field form :first_name) :last-name (field form :last_name)
                               :email (field form :email)}])))
(defn normalized [value]
  (cond
    (map? value) (into {} (map (fn [[key value]] [(str/replace (name key) "-" "_") (normalized value)]) value))
    (keyword? value) (str/capitalize (name value))
    (vector? value) (case (first value)
                      :none nil :some (normalized (second value))
                      :ok {"Ok" (normalized (second value))} :err {"Err" (normalized (second value))}
                      (mapv normalized value))
    :else value))
(let [scenario (json/read-str (slurp (first *command-line-args*)) :key-fn keyword)
      _ (reset! storage/initial {:next-id (:next_id scenario) :users {} :departments {}})
      db (app/unwrap (app/open ":memory:"))]
  (doseq [input (:operations scenario)]
    (when-let [message (:fail_next input)] (swap! storage/state assoc :fail-next message))
    (let [result (app/request db (request input))]
      (println (json/write-str {:result (normalized result) :changes (app/changes db) :trace (:trace @storage/state)}))))
  (app/close db)
  (binding [*out* *err*] (println (json/write-str {:clojure (clojure-version) :opened (:opened @storage/state) :closed (:closed @storage/state)}))))
