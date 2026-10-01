;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn ^number m3-fmix [h1 len]
  (as-> (int h1) h1
    (bit-xor h1 len)
    (bit-xor h1 (unsigned-bit-shift-right h1 16))
    (imul h1 (int 0x85ebca6b))
    (bit-xor h1 (unsigned-bit-shift-right h1 13))
    (imul h1 (int 0xc2b2ae35))
    (bit-xor h1 (unsigned-bit-shift-right h1 16))))
