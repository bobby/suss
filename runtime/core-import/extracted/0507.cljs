;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn- find-and-cache-best-method
  [name dispatch-val hierarchy method-table prefer-table method-cache cached-hierarchy default-dispatch-val]
  (let [best-entry (reduce (fn [be [k _ :as e]]
                             (if (isa? @hierarchy dispatch-val k)
                               (let [be2 (if (or (nil? be) (dominates k (first be) prefer-table @hierarchy))
                                           e
                                           be)]
                                 (when-not (dominates (first be2) k prefer-table @hierarchy)
                                   (throw (js/Error.
                                            (str_ "Multiple methods in multimethod '" name
                                              "' match dispatch value: " dispatch-val " -> " k
                                              " and " (first be2) ", and neither is preferred"))))
                                 be2)
                               be))
                     nil @method-table)
        best-entry (if-let [entry (and (nil? best-entry) (@method-table default-dispatch-val))]
                     [default-dispatch-val entry]
                     best-entry)]
    (when best-entry
      (if (= @cached-hierarchy @hierarchy)
        (do
          (swap! method-cache assoc dispatch-val (second best-entry))
          (second best-entry))
        (do
          (reset-cache method-cache method-table cached-hierarchy hierarchy)
          (find-and-cache-best-method name dispatch-val hierarchy method-table prefer-table
            method-cache cached-hierarchy default-dispatch-val))))))
