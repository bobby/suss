;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn keyword
  "Returns a Keyword with the given namespace and name.  Do not use :
  in the keyword strings, it will be added automatically."
  ([name] (cond
            (keyword? name) name
            (symbol? name) (Keyword.
                             (cljs.core/namespace name)
                             (cljs.core/name name) (.-str name) nil)
            (= "/" name) (Keyword. nil name name nil)
            (string? name) (let [parts (.split name "/")]
                             (if (== (alength parts) 2)
                               (Keyword. (aget parts 0) (aget parts 1) name nil)
                               (Keyword. nil (aget parts 0) name nil)))))
  ([ns name]
   (let [ns   (cond
                (keyword? ns) (cljs.core/name ns)
                (symbol? ns)  (cljs.core/name ns)
                :else ns)
         name (cond
                (keyword? name) (cljs.core/name name)
                (symbol? name) (cljs.core/name name)
                :else name)]
     (Keyword. ns name (str_ (when ns (str_ ns "/")) name) nil))))
