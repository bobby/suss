(defn fn-self-name [{:keys [name info] :as name-var}]
  (let [name (string/replace (str name) ".." "_DOT__DOT_")
        {:keys [ns fn-scope]} info
        scoped-name (apply str
                      (interpose "_$_"
                        (concat (map (comp str :name) fn-scope) [name])))]
    (symbol
      (munge
        (str (string/replace (str ns) "." "$") "$" scoped-name)))))
