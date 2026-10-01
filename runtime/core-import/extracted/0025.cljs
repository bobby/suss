;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defprotocol IDrop
  "Protocol for persistent or algorithmically defined collections to provide a
  means of dropping N items that is more efficient than sequential walking."
  (^clj-or-nil -drop [coll n]
    "Returns a collection that is ISequential, ISeq, and IReduce, or nil if past
     the end. The number of items to drop n must be > 0. It is also useful if the
     returned coll implements IDrop for subsequent use in a partition-like scenario."))
