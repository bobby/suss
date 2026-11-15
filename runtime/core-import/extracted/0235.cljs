;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn- inode-kv-reduce [arr f init]
  (let [len (alength arr)]
    (loop [i 0 init init]
      (if (< i len)
        (let [init (let [k (aget arr i)]
                     (if-not (nil? k)
                       (f init k (aget arr (inc i)))
                       (let [node (aget arr (inc i))]
                         (if-not (nil? node)
                           (.kv-reduce node f init)
                           init))))]
          (if (reduced? init)
            init
            (recur (+ i 2) init)))
        init))))
