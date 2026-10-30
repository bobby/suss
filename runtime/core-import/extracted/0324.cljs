;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(deftype PersistentQueueIter [^:mutable fseq riter]
  Object
  (hasNext [_]
    (or (and (some? fseq) (seq fseq)) (and (some? riter) (.hasNext riter))))
  (next [_]
    (cond
      (some? fseq)
      (let [ret (first fseq)]
        (set! fseq (next fseq))
        ret)
      (and (some? riter) ^boolean (.hasNext riter))
      (.next riter)
      :else (throw (js/Error. "No such element"))))
  (remove [_] (js/Error. "Unsupported operation")))
