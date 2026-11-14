;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn- create-array-node-seq
  ([nodes] (create-array-node-seq nodes 0 nil))
  ([nodes i s]
     (if (nil? s)
       (let [len (alength nodes)]
         (loop [j i]
           (if (< j len)
             (if-let [nj (aget nodes j)]
               (if-let [ns (.inode-seq nj)]
                 (ArrayNodeSeq. nil nodes (inc j) ns nil)
                 (recur (inc j)))
               (recur (inc j))))))
       (ArrayNodeSeq. nil nodes i s nil))))
