(ns suss-oracle.implementation-referrer
  (:require [suss-oracle.implementation-provider :as p :refer [implements?]]))

(defprotocol ReferrerProbe (read-referrer [x]))
(deftype ReferrerItem [] ReferrerProbe (read-referrer [x] 1))

(defn verify-refers []
  (and (true? (implements? ReferrerProbe (ReferrerItem.)))
       (= 77 (p/implements? ReferrerProbe (ReferrerItem.)))))
