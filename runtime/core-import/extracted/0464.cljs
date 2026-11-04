;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn interpose
  "Returns a lazy seq of the elements of coll separated by sep.
  Returns a stateful transducer when no collection is provided."
  ([sep]
    (fn [rf]
      (let [started (volatile! false)]
        (fn
          ([] (rf))
          ([result] (rf result))
          ([result input]
            (if @started
              (let [sepr (rf result sep)]
                (if (reduced? sepr)
                  sepr
                  (rf sepr input)))
              (do
                (vreset! started true)
                (rf result input))))))))
  ([sep coll] (drop 1 (interleave (repeat sep) coll))))
