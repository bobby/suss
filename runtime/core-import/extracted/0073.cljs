;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn- array-reduce
  ([arr f]
     (let [cnt (alength arr)]
       (if (zero? (alength arr))
         (f)
         (loop [val (aget arr 0), n 1]
           (if (< n cnt)
             (let [nval (f val (aget arr n))]
               (if (reduced? nval)
                 @nval
                 (recur nval (inc n))))
             val)))))
  ([arr f val]
     (let [cnt (alength arr)]
       (loop [val val, n 0]
         (if (< n cnt)
           (let [nval (f val (aget arr n))]
             (if (reduced? nval)
               @nval
               (recur nval (inc n))))
           val))))
  ([arr f val idx]
     (let [cnt (alength arr)]
       (loop [val val, n idx]
         (if (< n cnt)
           (let [nval (f val (aget arr n))]
             (if (reduced? nval)
               @nval
               (recur nval (inc n))))
           val)))))
