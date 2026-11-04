;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(deftype NodeIterator [arr ^:mutable i ^:mutable next-entry ^:mutable next-iter]
  Object
  (advance [this]
    (let [len (alength arr)]
      (loop []
        (if (< i len)
          (let [key (aget arr i)
                node-or-val (aget arr (inc i))
                ^boolean found
                (cond (some? key)
                      (set! next-entry (MapEntry. key node-or-val nil))
                      (some? node-or-val)
                      (let [new-iter (-iterator node-or-val)]
                        (if ^boolean (.hasNext new-iter)
                          (set! next-iter new-iter)
                          false))
                      :else false)]
            (set! i (+ i 2))
            (if found true (recur)))
          false))))
  (hasNext [this]
    (or (some? next-entry) (some? next-iter) (.advance this)))
  (next [this]
    (cond
      (some? next-entry)
      (let [ret next-entry]
        (set! next-entry nil)
        ret)
      (some? next-iter)
      (let [ret (.next next-iter)]
        (when-not ^boolean (.hasNext next-iter)
          (set! next-iter nil))
        ret)
      ^boolean (.advance this)
      (.next this)
      :else (throw (js/Error. "No such element"))))
  (remove [_] (js/Error. "Unsupported operation")))
