;   Copyright (c) Rich Hickey. All rights reserved.
;   The use and distribution terms for this software are covered by the
;   Eclipse Public License 1.0 (http://opensource.org/licenses/eclipse-1.0.php)
;   which can be found in the file epl-v10.html at the root of this distribution.
;   By using this software in any fashion, you are agreeing to be bound by
;   the terms of this license.
;   You must not remove this notice, or any other, from this software.

(defn- edit-and-set
  ([inode edit i a]
     (let [editable (.ensure-editable inode edit)]
       (aset (.-arr editable) i a)
       editable))
  ([inode edit i a j b]
     (let [editable (.ensure-editable inode edit)]
       (aset (.-arr editable) i a)
       (aset (.-arr editable) j b)
       editable)))
