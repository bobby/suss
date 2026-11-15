;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(deftype MetaFn [afn meta]
  IMeta
  (-meta [_] meta)
  IWithMeta
  (-with-meta [_ new-meta]
    (MetaFn. afn new-meta))
  Fn
  IFn
  (-invoke [_]
    (afn))
  (-invoke [_ a]
    (afn a))
  (-invoke [_ a b]
    (afn a b))
  (-invoke [_ a b c]
    (afn a b c))
  (-invoke [_ a b c d]
    (afn a b c d))
  (-invoke [_ a b c d e]
    (afn a b c d e))
  (-invoke [_ a b c d e f]
    (afn a b c d e f))
  (-invoke [_ a b c d e f g]
    (afn a b c d e f g))
  (-invoke [_ a b c d e f g h]
    (afn a b c d e f g h))
  (-invoke [_ a b c d e f g h i]
    (afn a b c d e f g h i))
  (-invoke [_ a b c d e f g h i j]
    (afn a b c d e f g h i j))
  (-invoke [_ a b c d e f g h i j k]
    (afn a b c d e f g h i j k))
  (-invoke [_ a b c d e f g h i j k l]
    (afn a b c d e f g h i j k l))
  (-invoke [_ a b c d e f g h i j k l m]
    (afn a b c d e f g h i j k l m))
  (-invoke [_ a b c d e f g h i j k l m n]
    (afn a b c d e f g h i j k l m n))
  (-invoke [_ a b c d e f g h i j k l m n o]
    (afn a b c d e f g h i j k l m n o))
  (-invoke [_ a b c d e f g h i j k l m n o p]
    (afn a b c d e f g h i j k l m n o p))
  (-invoke [_ a b c d e f g h i j k l m n o p q]
    (afn a b c d e f g h i j k l m n o p q))
  (-invoke [_ a b c d e f g h i j k l m n o p q r]
    (afn a b c d e f g h i j k l m n o p q r))
  (-invoke [_ a b c d e f g h i j k l m n o p q r s]
    (afn a b c d e f g h i j k l m n o p q r s))
  (-invoke [_ a b c d e f g h i j k l m n o p q r s t]
    (afn a b c d e f g h i j k l m n o p q r s t))
  (-invoke [_ a b c d e f g h i j k l m n o p q r s t rest]
    (apply afn a b c d e f g h i j k l m n o p q r s t rest)))
