;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn- pam-new-size [arr]
  (loop [i 0 n 0]
    (if (< i (alength arr))
      (let [dupe? (loop [j 0]
                    (if (< j i)
                      (or
                        (key-test (aget arr i) (aget arr j))
                        (recur (+ 2 j)))
                      false))]
        (recur (+ 2 i) (if dupe? n (+ n 2))))
      n)))
