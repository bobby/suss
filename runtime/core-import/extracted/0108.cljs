;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn- push-tail [pv level parent tailnode]
  (let [ret (pv-clone-node parent)
        subidx (bit-and (bit-shift-right-zero-fill (dec (.-cnt pv)) level) 0x01f)]
    (if (== 5 level)
      (do
        (pv-aset ret subidx tailnode)
        ret)
      (let [child (pv-aget parent subidx)]
        (if-not (nil? child)
          (let [node-to-insert (push-tail pv (- level 5) child tailnode)]
            (pv-aset ret subidx node-to-insert)
            ret)
          (let [node-to-insert (new-path nil (- level 5) tailnode)]
            (pv-aset ret subidx node-to-insert)
            ret))))))
