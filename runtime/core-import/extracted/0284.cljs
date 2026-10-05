;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(deftype Var [val sym _meta]
  Object
  (isMacro [_]
    (. (val) -cljs$lang$macro))
  (toString [_]
    (str_ "#'" sym))
  IDeref
  (-deref [_] (val))
  IMeta
  (-meta [_] _meta)
  IWithMeta
  (-with-meta [_ new-meta]
    (Var. val sym new-meta))
  IEquiv
  (-equiv [this other]
    (if (instance? Var other)
      (= (.-sym this) (.-sym other))
      false))
  IHash
  (-hash [_]
    (hash-symbol sym))
  Fn
  IFn
  (-invoke [_]
    ((val)))
  (-invoke [_ a]
    ((val) a))
  (-invoke [_ a b]
    ((val) a b))
  (-invoke [_ a b c]
    ((val) a b c))
  (-invoke [_ a b c d]
    ((val) a b c d))
  (-invoke [_ a b c d e]
    ((val) a b c d e))
  (-invoke [_ a b c d e f]
    ((val) a b c d e f))
  (-invoke [_ a b c d e f g]
    ((val) a b c d e f g))
  (-invoke [_ a b c d e f g h]
    ((val) a b c d e f g h))
  (-invoke [_ a b c d e f g h i]
    ((val) a b c d e f g h i))
  (-invoke [_ a b c d e f g h i j]
    ((val) a b c d e f g h i j))
  (-invoke [_ a b c d e f g h i j k]
    ((val) a b c d e f g h i j k))
  (-invoke [_ a b c d e f g h i j k l]
    ((val) a b c d e f g h i j k l))
  (-invoke [_ a b c d e f g h i j k l m]
    ((val) a b c d e f g h i j k l m))
  (-invoke [_ a b c d e f g h i j k l m n]
    ((val) a b c d e f g h i j k l m n))
  (-invoke [_ a b c d e f g h i j k l m n o]
    ((val) a b c d e f g h i j k l m n o))
  (-invoke [_ a b c d e f g h i j k l m n o p]
    ((val) a b c d e f g h i j k l m n o p))
  (-invoke [_ a b c d e f g h i j k l m n o p q]
    ((val) a b c d e f g h i j k l m n o p q))
  (-invoke [_ a b c d e f g h i j k l m n o p q r]
    ((val) a b c d e f g h i j k l m n o p q r))
  (-invoke [_ a b c d e f g h i j k l m n o p q r s]
    ((val) a b c d e f g h i j k l m n o p q r s))
  (-invoke [_ a b c d e f g h i j k l m n o p q r s t]
    ((val) a b c d e f g h i j k l m n o p q r s t))
  (-invoke [_ a b c d e f g h i j k l m n o p q r s t rest]
    (apply (val) a b c d e f g h i j k l m n o p q r s t rest)))
