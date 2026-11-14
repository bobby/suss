;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn- create-node
  ([shift key1 val1 key2hash key2 val2]
     (let [key1hash (hash key1)]
       (if (== key1hash key2hash)
         (HashCollisionNode. nil key1hash 2 (array key1 val1 key2 val2))
         (let [added-leaf? (Box. false)]
           (-> (.-EMPTY BitmapIndexedNode)
               (.inode-assoc shift key1hash key1 val1 added-leaf?)
               (.inode-assoc shift key2hash key2 val2 added-leaf?))))))
  ([edit shift key1 val1 key2hash key2 val2]
     (let [key1hash (hash key1)]
       (if (== key1hash key2hash)
         (HashCollisionNode. nil key1hash 2 (array key1 val1 key2 val2))
         (let [added-leaf? (Box. false)]
           (-> (.-EMPTY BitmapIndexedNode)
               (.inode-assoc! edit shift key1hash key1 val1 added-leaf?)
               (.inode-assoc! edit shift key2hash key2 val2 added-leaf?)))))))
