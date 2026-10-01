;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn hash-double [f]
  (let [arr  (doto (js/Float64Array. 1) (aset 0 f))
        buf  (.-buffer arr)
        high (.getInt32 (js/DataView. buf 0 4))
        low  (.getInt32 (js/DataView. buf 4 4))]
    (hash-long high low)))
