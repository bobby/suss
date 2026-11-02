;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn replace
  "Given a map of replacement pairs and a vector/collection, returns a
  vector/seq with any elements = a key in smap replaced with the
  corresponding val in smap.  Returns a transducer when no collection
  is provided."
  ([smap]
     (map #(if-let [e (find smap %)] (val e) %)))
  ([smap coll]
     (if (vector? coll)
       (let [n (count coll)]
         (reduce (fn [v i]
                   (if-let [e (find smap (nth v i))]
                     (assoc v i (second e))
                     v))
           coll (take n (iterate inc 0))))
       (map #(if-let [e (find smap %)] (second e) %) coll))))
