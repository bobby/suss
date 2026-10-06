;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn symbol
  "Returns a Symbol with the given namespace and name. Arity-1 works
  on strings, keywords, and vars."
  ([name]
   (cond (symbol? name) name
         (string? name) (let [idx (.indexOf name "/")]
                          (if (< idx 1)
                            (symbol nil name)
                            (symbol (.substring name 0 idx)
                                    (.substring name (inc idx) (. name -length)))))
         (var? name) (.-sym name)
         (keyword? name) (recur (.-fqn name))
         :else (throw (new js/Error "no conversion to symbol"))))
  ([ns name]
   (let [sym-str (if-not (nil? ns)
                   (str_ ns "/" name)
                   name)]
     (Symbol. ns name sym-str nil nil))))
