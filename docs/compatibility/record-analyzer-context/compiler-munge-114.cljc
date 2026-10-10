(defn munge
  ([s] (munge s js-reserved))
  ([s reserved]
   (if #?(:clj  (map? s)
          :cljs (ana.impl/cljs-map? s))
     (let [name-var s
           name     (:name name-var)
           field    (:field name-var)
           info     (:info name-var)]
       (if-not (nil? (:fn-self-name info))
         (fn-self-name s)
         ;; Unshadowing
         (let [depth       (shadow-depth s)
               code        (hash-scope s)
               renamed     (get *lexical-renames* code)
               name        (cond
                             (true? field) (str "self__." name)
                             (not (nil? renamed)) renamed
                             :else name)
               munged-name (munge name reserved)]
           (if (or (true? field) (zero? depth))
             munged-name
             (symbol (str munged-name "__$" depth))))))
     ;; String munging
     (let [ss (string/replace (str s) ".." "_DOT__DOT_")
           ss (string/replace ss
                #?(:clj #"\/(.)" :cljs (js/RegExp. "\\/(.)")) ".$1") ; Division is special
           rf (munge-reserved reserved)
           ss (map rf (string/split ss #"\."))
           ss (string/join "." ss)
           ms #?(:clj (clojure.lang.Compiler/munge ss)
                 :cljs (munge-str ss))]
       (if (symbol? s)
         (symbol ms)
         ms)))))
