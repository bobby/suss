;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn- pr-writer-impl
  [obj writer opts]
  (cond
    (nil? obj) (-write writer "nil")
    :else
    (do
      (when (print-meta? opts obj)
        (-write writer "^")
        (pr-writer (meta obj) writer opts)
        (-write writer " "))
      (cond
        ;; FIXME: can we figure out something better here?
        ;; handle CLJS ctors
        ^boolean (.-cljs$lang$type obj)
        (.cljs$lang$ctorPrWriter obj obj writer opts)

        ; Use the new, more efficient, IPrintWithWriter interface when possible.
        (satisfies? IPrintWithWriter obj)
        (-pr-writer obj writer opts)

        (or (true? obj) (false? obj))
        (-write writer (str_ obj))

        (number? obj)
        (-write writer
          (cond
            (js/isNaN obj) "##NaN"
            (identical? obj js/Number.POSITIVE_INFINITY) "##Inf"
            (identical? obj js/Number.NEGATIVE_INFINITY) "##-Inf"
            :else (str_ obj)))

        (object? obj)
        (do
          (-write writer "#js ")
          (print-map
            (.map
              (js-keys obj)
              (fn [k]
                (MapEntry.
                  (cond-> k (some? (.match k #"^[A-Za-z_\*\+\?!\-'][\w\*\+\?!\-']*$")) keyword)
                  (unchecked-get obj k)
                  nil)))
            pr-writer writer opts))

        (array? obj)
        (pr-sequential-writer writer pr-writer "#js [" " " "]" opts obj)

        (string? obj)
        (if (pr-opts-readably opts)
          (-write writer (quote-string obj))
          (-write writer obj))

        (js-fn? obj)
        (let [name (.-name obj)
              name (if (or (nil? name) (gstring/isEmpty name))
                     "Function"
                     name)]
          (write-all writer "#object[" name
            (if *print-fn-bodies*
              (str_ " \"" (str_ obj) "\"")
              "")
            "]"))

        (instance? js/Date obj)
        (let [normalize (fn [n len]
                          (loop [ns (str_ n)]
                            (if (< (count ns) len)
                              (recur (str_ "0" ns))
                              ns)))]
          (write-all writer
            "#inst \""
            (normalize (.getUTCFullYear obj) 4)     "-"
            (normalize (inc (.getUTCMonth obj)) 2)  "-"
            (normalize (.getUTCDate obj) 2)         "T"
            (normalize (.getUTCHours obj) 2)        ":"
            (normalize (.getUTCMinutes obj) 2)      ":"
            (normalize (.getUTCSeconds obj) 2)      "."
            (normalize (.getUTCMilliseconds obj) 3) "-"
            "00:00\""))

        (regexp? obj) (write-all writer "#\"" (.-source obj) "\"")

        (js-symbol? obj) (write-all writer "#object[" (.toString obj) "]" )

        :else
        (if (some-> obj .-constructor .-cljs$lang$ctorStr)
          (write-all writer
            "#object[" (.replace (.. obj -constructor -cljs$lang$ctorStr)
                         (js/RegExp. "/" "g") ".") "]")
          (let [name (some-> obj .-constructor .-name)
                name (if (or (nil? name) (gstring/isEmpty name))
                       "Object"
                       name)]
            (if (nil? (. obj -constructor))
              (write-all writer "#object[" name "]")
              (write-all writer "#object[" name " " (str_ obj) "]"))))))))
