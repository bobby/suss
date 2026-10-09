;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn- tree-map-kv-reduce [node f init]
  (let [init (if-not (nil? (.-left node))
               (tree-map-kv-reduce (.-left node) f init)
               init)]
    (if (reduced? init)
      init
      (let [init (f init (.-key node) (.-val node))]
        (if (reduced? init)
          init
          (if-not (nil? (.-right node))
            (tree-map-kv-reduce (.-right node) f init)
            init))))))
