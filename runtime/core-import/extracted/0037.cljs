;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn ^number m3-hash-int [in]
  (if (zero? in)
    in
    (let [k1 (m3-mix-K1 in)
          h1 (m3-mix-H1 m3-seed k1)]
      (m3-fmix h1 4))))
