# Primary Ratio observations

oracle.clj and cases.json contain22 fresh JVM Clojure1.12.6 observations for
Ratio normalization, arithmetic, quot/rem/mod, numerator/denominator, equality,
hashing, conversion and rationalize. oracle-command.json retains the exact
command, JAR and source hashes and terminal status. These are expected values;
they do not certify any Suss Ratio execution.

Ratio.javap.txt records the installed primary class's methods. In particular,
doubleValue uses MathContext.DECIMAL64 through decimalValue, then converts the
result to binary64. The context has16 decimal digits with HALF_EVEN rounding;
direct binary64 division is not that behavior. This is a development oracle,
not a shipped JVM dependency. Clojure1.12.6 is EPL1.0; imported runtime core
packaging retains that license separately.
