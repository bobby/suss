;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(deftype MultiFn [name dispatch-fn default-dispatch-val hierarchy
                  method-table prefer-table method-cache cached-hierarchy]
  IFn
  (-invoke [mf]
    (let [dispatch-val (dispatch-fn)
          target-fn (-get-method mf dispatch-val)]
      (when-not target-fn
        (throw-no-method-error name dispatch-val))
      (target-fn)))
  (-invoke [mf a]
    (let [dispatch-val (dispatch-fn a)
          target-fn (-get-method mf dispatch-val)]
      (when-not target-fn
        (throw-no-method-error name dispatch-val))
      (target-fn a)))
  (-invoke [mf a b]
    (let [dispatch-val (dispatch-fn a b)
          target-fn (-get-method mf dispatch-val)]
      (when-not target-fn
        (throw-no-method-error name dispatch-val))
      (target-fn a b)))
  (-invoke [mf a b c]
    (let [dispatch-val (dispatch-fn a b c)
          target-fn (-get-method mf dispatch-val)]
      (when-not target-fn
        (throw-no-method-error name dispatch-val))
      (target-fn a b c)))
  (-invoke [mf a b c d]
    (let [dispatch-val (dispatch-fn a b c d)
          target-fn (-get-method mf dispatch-val)]
      (when-not target-fn
        (throw-no-method-error name dispatch-val))
      (target-fn a b c d)))
  (-invoke [mf a b c d e]
    (let [dispatch-val (dispatch-fn a b c d e)
          target-fn (-get-method mf dispatch-val)]
      (when-not target-fn
        (throw-no-method-error name dispatch-val))
      (target-fn a b c d e)))
  (-invoke [mf a b c d e f]
    (let [dispatch-val (dispatch-fn a b c d e f)
          target-fn (-get-method mf dispatch-val)]
      (when-not target-fn
        (throw-no-method-error name dispatch-val))
      (target-fn a b c d e f)))
  (-invoke [mf a b c d e f g]
    (let [dispatch-val (dispatch-fn a b c d e f g)
          target-fn (-get-method mf dispatch-val)]
      (when-not target-fn
        (throw-no-method-error name dispatch-val))
      (target-fn a b c d e f g)))
  (-invoke [mf a b c d e f g h]
    (let [dispatch-val (dispatch-fn a b c d e f g h)
          target-fn (-get-method mf dispatch-val)]
      (when-not target-fn
        (throw-no-method-error name dispatch-val))
      (target-fn a b c d e f g h)))
  (-invoke [mf a b c d e f g h i]
    (let [dispatch-val (dispatch-fn a b c d e f g h i)
          target-fn (-get-method mf dispatch-val)]
      (when-not target-fn
        (throw-no-method-error name dispatch-val))
      (target-fn a b c d e f g h i)))
  (-invoke [mf a b c d e f g h i j]
    (let [dispatch-val (dispatch-fn a b c d e f g h i j)
          target-fn (-get-method mf dispatch-val)]
      (when-not target-fn
        (throw-no-method-error name dispatch-val))
      (target-fn a b c d e f g h i j)))
  (-invoke [mf a b c d e f g h i j k]
    (let [dispatch-val (dispatch-fn a b c d e f g h i j k)
          target-fn (-get-method mf dispatch-val)]
      (when-not target-fn
        (throw-no-method-error name dispatch-val))
      (target-fn a b c d e f g h i j k)))
  (-invoke [mf a b c d e f g h i j k l]
    (let [dispatch-val (dispatch-fn a b c d e f g h i j k l)
          target-fn (-get-method mf dispatch-val)]
      (when-not target-fn
        (throw-no-method-error name dispatch-val))
      (target-fn a b c d e f g h i j k l)))
  (-invoke [mf a b c d e f g h i j k l m]
    (let [dispatch-val (dispatch-fn a b c d e f g h i j k l m)
          target-fn (-get-method mf dispatch-val)]
      (when-not target-fn
        (throw-no-method-error name dispatch-val))
      (target-fn a b c d e f g h i j k l m)))
  (-invoke [mf a b c d e f g h i j k l m n]
    (let [dispatch-val (dispatch-fn a b c d e f g h i j k l m n)
          target-fn (-get-method mf dispatch-val)]
      (when-not target-fn
        (throw-no-method-error name dispatch-val))
      (target-fn a b c d e f g h i j k l m n)))
  (-invoke [mf a b c d e f g h i j k l m n o]
    (let [dispatch-val (dispatch-fn a b c d e f g h i j k l m n o)
          target-fn (-get-method mf dispatch-val)]
      (when-not target-fn
        (throw-no-method-error name dispatch-val))
      (target-fn a b c d e f g h i j k l m n o)))
  (-invoke [mf a b c d e f g h i j k l m n o p]
    (let [dispatch-val (dispatch-fn a b c d e f g h i j k l m n o p)
          target-fn (-get-method mf dispatch-val)]
      (when-not target-fn
        (throw-no-method-error name dispatch-val))
      (target-fn a b c d e f g h i j k l m n o p)))
  (-invoke [mf a b c d e f g h i j k l m n o p q]
    (let [dispatch-val (dispatch-fn a b c d e f g h i j k l m n o p q)
          target-fn (-get-method mf dispatch-val)]
      (when-not target-fn
        (throw-no-method-error name dispatch-val))
      (target-fn a b c d e f g h i j k l m n o p q)))
  (-invoke [mf a b c d e f g h i j k l m n o p q r]
    (let [dispatch-val (dispatch-fn a b c d e f g h i j k l m n o p q r)
          target-fn (-get-method mf dispatch-val)]
      (when-not target-fn
        (throw-no-method-error name dispatch-val))
      (target-fn a b c d e f g h i j k l m n o p q r)))
  (-invoke [mf a b c d e f g h i j k l m n o p q r s]
    (let [dispatch-val (dispatch-fn a b c d e f g h i j k l m n o p q r s)
          target-fn (-get-method mf dispatch-val)]
      (when-not target-fn
        (throw-no-method-error name dispatch-val))
      (target-fn a b c d e f g h i j k l m n o p q r s)))
  (-invoke [mf a b c d e f g h i j k l m n o p q r s t]
    (let [dispatch-val (dispatch-fn a b c d e f g h i j k l m n o p q r s t)
          target-fn (-get-method mf dispatch-val)]
      (when-not target-fn
        (throw-no-method-error name dispatch-val))
      (target-fn a b c d e f g h i j k l m n o p q r s t)))
  (-invoke [mf a b c d e f g h i j k l m n o p q r s t rest]
    (let [dispatch-val (apply dispatch-fn a b c d e f g h i j k l m n o p q r s t rest)
          target-fn (-get-method mf dispatch-val)]
      (when-not target-fn
        (throw-no-method-error name dispatch-val))
      (apply target-fn a b c d e f g h i j k l m n o p q r s t rest)))

  IMultiFn
  (-reset [mf]
    (swap! method-table (fn [mf] {}))
    (swap! method-cache (fn [mf] {}))
    (swap! prefer-table (fn [mf] {}))
    (swap! cached-hierarchy (fn [mf] nil))
    mf)

  (-add-method [mf dispatch-val method]
    (swap! method-table assoc dispatch-val method)
    (reset-cache method-cache method-table cached-hierarchy hierarchy)
    mf)

  (-remove-method [mf dispatch-val]
    (swap! method-table dissoc dispatch-val)
    (reset-cache method-cache method-table cached-hierarchy hierarchy)
    mf)

  (-get-method [mf dispatch-val]
    (when-not (= @cached-hierarchy @hierarchy)
      (reset-cache method-cache method-table cached-hierarchy hierarchy))
    (if-let [target-fn (@method-cache dispatch-val)]
      target-fn
      (find-and-cache-best-method name dispatch-val hierarchy method-table
        prefer-table method-cache cached-hierarchy default-dispatch-val)))

  (-prefer-method [mf dispatch-val-x dispatch-val-y]
    (when (prefers* dispatch-val-y dispatch-val-x  prefer-table)
      (throw (js/Error. (str_ "Preference conflict in multimethod '" name "': " dispatch-val-y
                   " is already preferred to " dispatch-val-x))))
    (swap! prefer-table
           (fn [old]
             (assoc old dispatch-val-x
                    (conj (get old dispatch-val-x #{})
                          dispatch-val-y))))
    (reset-cache method-cache method-table cached-hierarchy hierarchy))

  (-methods [mf] @method-table)
  (-prefers [mf] @prefer-table)
  (-default-dispatch-val [mf] default-dispatch-val)
  (-dispatch-fn [mf] dispatch-fn)

  INamed
  (-name [this] (-name name))
  (-namespace [this] (-namespace name))

  IHash
  (-hash [this] (goog/getUid this)))
