;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn transformer-iterator
  [xform sourceIter multi]
  (let [iterator (TransformerIterator. EMPTY NONE false nil sourceIter multi)]
    (set! (.-xf iterator)
      (xform (fn
               ([] nil)
               ([acc] acc)
               ([acc o]
                (set! (.-buffer iterator) (.add (.-buffer iterator) o))
                acc))))
    iterator))
