(assert (= "1.12.6" (clojure-version)))
(prn (let [outcome (fn [source]
                (try (eval source) :accepted (catch Throwable e :rejected)))]
 [(outcome '(def ^{:arglists {:arity [unbound-x]}} bad-arglists 1))
  (outcome '(def ^{:top-fn {:method-params [[unbound-override]]}} bad-top-fn 1))
  (outcome '(def ^unbound-type bad-tag 1))
  (outcome '(def ^{:name unbound-name :ns unbound-ns} bad-identity 1))
  (eval '(let [x 'x override 'override]
           (def ^{:arglists {:arity [x]}
                  :top-fn {:method-params [[override]] :arglists [[override]]}}
             bound-params (fn [x] x))
           (select-keys (meta #'bound-params) [:arglists :top-fn])))
  (eval '(let [string 'string number 'number forged 'forged wrong 'wrong]
           (def ^string bound-hint 1)
           (def ^{:name forged :ns wrong :tag number} bound-identities 3)
           [(:tag (meta #'bound-hint)) (:tag (meta #'bound-identities))
            (= 'bound-identities (:name (meta #'bound-identities)))
            (= *ns* (:ns (meta #'bound-identities)))]))]))
