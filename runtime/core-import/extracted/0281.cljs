;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn- build-subvec [meta v start end __hash]
  (if (instance? Subvec v)
    (recur meta (.-v v) (+ (.-start v) start) (+ (.-start v) end) __hash)
    (do
      (when-not (vector? v)
        (throw (js/Error. "v must satisfy IVector")))
      (when (or (neg? start)
                (< end start)
                (> end (count v)))
        (throw (js/Error. "Index out of bounds")))
      (Subvec. meta v start end __hash))))
