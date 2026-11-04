;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn- lift-ns
  "Returns #js [lifted-ns lifted-map] or nil if m can't be lifted."
  [m]
  (when *print-namespace-maps*
    (let [lm #js []]
      (loop [ns nil
             [[k v :as entry] & entries] (seq m)]
        (if entry
          (when (or (keyword? k) (symbol? k))
            (if ns
              (when (= ns (namespace k))
                (.push lm (MapEntry. (strip-ns k) v nil))
                (recur ns entries))
              (when-let [new-ns (namespace k)]
                (.push lm (MapEntry. (strip-ns k) v nil))
                (recur new-ns entries))))
          #js [ns lm])))))
