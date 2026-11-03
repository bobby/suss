;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(deftype ArrayNodeIterator [arr ^:mutable i ^:mutable next-iter]
  Object
  (hasNext [this]
    (let [len (alength arr)]
      (loop []
        (if-not (and  (some? next-iter) ^boolean (.hasNext next-iter))
          (if (< i len)
            (let [node (aget arr i)]
              (set! i (inc i))
              (when (some? node)
                (set! next-iter (-iterator node)))
              (recur))
            false)
          true))))
  (next [this]
    (if ^boolean (.hasNext this)
      (.next next-iter)
      (throw (js/Error. "No such element"))))
  (remove [_] (js/Error. "Unsupported operation")))
