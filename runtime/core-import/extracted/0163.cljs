;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn get
  "Returns the value mapped to key, not-found or nil if key not present
  in associative collection, set, string, array, or ILookup instance."
  ([o k]
    (when-not (nil? o)
      (cond
        (implements? ILookup o)
        (-lookup o k)

        (array? o)
        (when (and (some? k) (< k (.-length o)))
          (aget o (int k)))

        (string? o)
        (when (and (some? k) (< -1 k (.-length o)))
          (.charAt o (int k)))

        (native-satisfies? ILookup o)
        (-lookup o k)

        :else nil)))
  ([o k not-found]
    (if-not (nil? o)
      (cond
        (implements? ILookup o)
        (-lookup o k not-found)

        (array? o)
        (if (and (some? k) (< -1 k (.-length o)))
          (aget o (int k))
          not-found)

        (string? o)
        (if (and (some? k) (< -1 k (.-length o)))
          (.charAt o (int k))
          not-found)

        (native-satisfies? ILookup o)
        (-lookup o k not-found)

        :else not-found)
      not-found)))
