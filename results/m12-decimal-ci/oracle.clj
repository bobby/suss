
(prn (pr-str 1M))

(prn (pr-str 1.50M))

(prn (pr-str -0.00M))

(prn (pr-str [1.50M -0.00M 123456789012345678901234567890.00M]))

(prn (let [x (read-string "1.50M")] [(pr-str x) (decimal? x) (rational? x) (number? x) (= (class x) BigDecimal)]))

(prn (let [x 1.50M] [(str (class x)) (pr-str (class x)) (instance? BigDecimal x) (instance? Number x) (ifn? (class x))]))

(prn (let [c (class 1M) trace (atom 0)] [(try (c (do (swap! trace inc) 7)) false (catch Throwable _ true)) @trace]))

(prn ((fn [x] [(pr-str x) (decimal? x) (= (type x) BigDecimal)]) 1.25M))
(def ^:const decimal-ci-constant 1.50M) (def decimal-ci-read (fn [] decimal-ci-constant)) (def decimal-ci-constant 2.50M)
(prn [(pr-str (decimal-ci-read)) (pr-str decimal-ci-constant)])
(defprotocol DecimalCiProtocol (decimal-ci-kind [x])) (extend-type java.math.BigDecimal DecimalCiProtocol (decimal-ci-kind [x] :decimal)) (extend-type java.lang.Number DecimalCiProtocol (decimal-ci-kind [x] :number))
(prn [(decimal-ci-kind 1M) (decimal-ci-kind 1) (decimal-ci-kind 1N) (decimal-ci-kind 1/3)])

(prn (let [x (read-string "0E2147483648M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "0E2147483647M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "0E100M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "0E7M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "0E6M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "0E1M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "0E0M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "0E-1M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "0E-2M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "0E-5M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "0E-6M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "0E-7M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "0E-10M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "0E-100M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "0E-2147483647M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "1E2147483648M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "1E2147483647M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "1E100M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "1E7M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "1E6M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "1E1M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "1E0M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "1E-1M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "1E-2M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "1E-5M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "1E-6M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "1E-7M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "1E-10M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "1E-100M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "1E-2147483647M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-1E2147483648M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-1E2147483647M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-1E100M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-1E7M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-1E6M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-1E1M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-1E0M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-1E-1M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-1E-2M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-1E-5M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-1E-6M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-1E-7M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-1E-10M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-1E-100M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-1E-2147483647M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "15E2147483648M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "15E2147483647M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "15E100M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "15E7M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "15E6M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "15E1M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "15E0M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "15E-1M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "15E-2M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "15E-5M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "15E-6M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "15E-7M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "15E-10M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "15E-100M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "15E-2147483647M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-150E2147483648M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-150E2147483647M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-150E100M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-150E7M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-150E6M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-150E1M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-150E0M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-150E-1M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-150E-2M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-150E-5M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-150E-6M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-150E-7M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-150E-10M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-150E-100M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-150E-2147483647M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "12300E2147483648M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "12300E2147483647M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "12300E100M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "12300E7M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "12300E6M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "12300E1M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "12300E0M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "12300E-1M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "12300E-2M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "12300E-5M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "12300E-6M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "12300E-7M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "12300E-10M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "12300E-100M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "12300E-2147483647M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "123456789012345678901234567890123400E2147483648M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "123456789012345678901234567890123400E2147483647M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "123456789012345678901234567890123400E100M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "123456789012345678901234567890123400E7M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "123456789012345678901234567890123400E6M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "123456789012345678901234567890123400E1M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "123456789012345678901234567890123400E0M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "123456789012345678901234567890123400E-1M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "123456789012345678901234567890123400E-2M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "123456789012345678901234567890123400E-5M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "123456789012345678901234567890123400E-6M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "123456789012345678901234567890123400E-7M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "123456789012345678901234567890123400E-10M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "123456789012345678901234567890123400E-100M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "123456789012345678901234567890123400E-2147483647M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "95E-1M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "95E-7M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "95E-8M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-21E-1M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-21E-7M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-21E-8M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "316E-2M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "316E-8M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "316E-9M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-896E-2M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-896E-8M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-896E-9M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "94197648E-7M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "94197648E-13M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "94197648E-14M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-76025839E-7M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-76025839E-13M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-76025839E-14M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "9553823694665801E-15M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "9553823694665801E-21M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "9553823694665801E-22M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-9815572503043468E-15M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-9815572503043468E-21M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-9815572503043468E-22M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "47095799874146675218276944743198E-31M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "47095799874146675218276944743198E-37M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "47095799874146675218276944743198E-38M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-81915403720881136581871943296964E-31M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-81915403720881136581871943296964E-37M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-81915403720881136581871943296964E-38M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "6405674557579655456395167491658650692172788449793812225175524675E-63M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "6405674557579655456395167491658650692172788449793812225175524675E-69M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "6405674557579655456395167491658650692172788449793812225175524675E-70M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-8333848679797353430767010344253765611782977263199235401534942659E-63M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-8333848679797353430767010344253765611782977263199235401534942659E-69M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-8333848679797353430767010344253765611782977263199235401534942659E-70M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "79249161532238215157454067174946652214500153028447664770745311063502479066761484545488478195023286136662584606685034265875711598E-127M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "79249161532238215157454067174946652214500153028447664770745311063502479066761484545488478195023286136662584606685034265875711598E-133M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "79249161532238215157454067174946652214500153028447664770745311063502479066761484545488478195023286136662584606685034265875711598E-134M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-72842620475162938858522727162258665870853165913870468735322655034562928010582979746121225489689334932792663516649958622659280364E-127M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-72842620475162938858522727162258665870853165913870468735322655034562928010582979746121225489689334932792663516649958622659280364E-133M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-72842620475162938858522727162258665870853165913870468735322655034562928010582979746121225489689334932792663516649958622659280364E-134M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "7244970540930600225602152557915820900178660053568065586555989843804741877721190117947037010204535625618290155673793395746727725085874058421235519889524884040756870931832437309824402305661410431140009385419989374652351962097555699357895462398210012495824278E-255M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "7244970540930600225602152557915820900178660053568065586555989843804741877721190117947037010204535625618290155673793395746727725085874058421235519889524884040756870931832437309824402305661410431140009385419989374652351962097555699357895462398210012495824278E-261M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "7244970540930600225602152557915820900178660053568065586555989843804741877721190117947037010204535625618290155673793395746727725085874058421235519889524884040756870931832437309824402305661410431140009385419989374652351962097555699357895462398210012495824278E-262M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-1786493216098314606884292497805482883990870636842039172977752725109592882750334192544972769064964008546896137495004764224297505326519799481455783405011210089313791446363897632610630784187556366740834428633702159329789154766297868966029690899871749213626948E-255M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-1786493216098314606884292497805482883990870636842039172977752725109592882750334192544972769064964008546896137495004764224297505326519799481455783405011210089313791446363897632610630784187556366740834428633702159329789154766297868966029690899871749213626948E-261M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-1786493216098314606884292497805482883990870636842039172977752725109592882750334192544972769064964008546896137495004764224297505326519799481455783405011210089313791446363897632610630784187556366740834428633702159329789154766297868966029690899871749213626948E-262M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "7244088418692618723149931114449618483317147871947106152967059750666063917812400169225287468938812319364706621656958625706484614894697104929371889071040347322989972490260261187523976965145283025487945199625063133011816723950082820723728302056213329936030444765688080788280343590985823054824365691513535018022855318314933108061015059906277938542101644696165207838705110007234337303098197760734214605576315645605266852892476136378259335411126795684292083788342880044707559201806009011834919124243890616440853788068432120356073648208457108132127564148084389610934850285076226939280226194065693017325591813883869220076388009197020313440520641614305902107331136363596486319893678348116142079367941726726591803026314452285186448805109537270485188721793647029082958246656039759176246080722205667544571292988593771830845214516887254788191278849440329904683639561652244139272115307661969297862936085485716462319089808958312404988869586299339635528284784573132749472231604199668668163864385132736589521420631767288799577199614349877149E-1023M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "7244088418692618723149931114449618483317147871947106152967059750666063917812400169225287468938812319364706621656958625706484614894697104929371889071040347322989972490260261187523976965145283025487945199625063133011816723950082820723728302056213329936030444765688080788280343590985823054824365691513535018022855318314933108061015059906277938542101644696165207838705110007234337303098197760734214605576315645605266852892476136378259335411126795684292083788342880044707559201806009011834919124243890616440853788068432120356073648208457108132127564148084389610934850285076226939280226194065693017325591813883869220076388009197020313440520641614305902107331136363596486319893678348116142079367941726726591803026314452285186448805109537270485188721793647029082958246656039759176246080722205667544571292988593771830845214516887254788191278849440329904683639561652244139272115307661969297862936085485716462319089808958312404988869586299339635528284784573132749472231604199668668163864385132736589521420631767288799577199614349877149E-1029M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "7244088418692618723149931114449618483317147871947106152967059750666063917812400169225287468938812319364706621656958625706484614894697104929371889071040347322989972490260261187523976965145283025487945199625063133011816723950082820723728302056213329936030444765688080788280343590985823054824365691513535018022855318314933108061015059906277938542101644696165207838705110007234337303098197760734214605576315645605266852892476136378259335411126795684292083788342880044707559201806009011834919124243890616440853788068432120356073648208457108132127564148084389610934850285076226939280226194065693017325591813883869220076388009197020313440520641614305902107331136363596486319893678348116142079367941726726591803026314452285186448805109537270485188721793647029082958246656039759176246080722205667544571292988593771830845214516887254788191278849440329904683639561652244139272115307661969297862936085485716462319089808958312404988869586299339635528284784573132749472231604199668668163864385132736589521420631767288799577199614349877149E-1030M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-6784325061858374798615545734827229586433692271286209510728266035607858608277366198740932853927042853948862032829815535710756625035065329708491921738865913890260989698108821310596099455147176953019923422348730679065428851037800849270547108511121248707185002403578355982679801575791994563195898932274805339948279833627022141883341299596410769543028163225689425273474490142560282137071525171497888083173015664630372923266915966653706541328243275859256986059390729305695659315020921915255407074869219980418217967722821817628304929281330813026821838560492455438109777136528930145917093617069866220166858365530531758581593884074962713021742495946027380566096663073172208810108062542839018708590656207600538488005680806061159817117937446423635181161090135903445427193662453949287670168906667040504057635833572325321609849875483379879503746586379571948616648440931782652980425872757672991175590086646394353887802616879562644198824690327989301513611998764748003811531167214679540635804391942069573888163886407070760389175334936581555E-1023M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-6784325061858374798615545734827229586433692271286209510728266035607858608277366198740932853927042853948862032829815535710756625035065329708491921738865913890260989698108821310596099455147176953019923422348730679065428851037800849270547108511121248707185002403578355982679801575791994563195898932274805339948279833627022141883341299596410769543028163225689425273474490142560282137071525171497888083173015664630372923266915966653706541328243275859256986059390729305695659315020921915255407074869219980418217967722821817628304929281330813026821838560492455438109777136528930145917093617069866220166858365530531758581593884074962713021742495946027380566096663073172208810108062542839018708590656207600538488005680806061159817117937446423635181161090135903445427193662453949287670168906667040504057635833572325321609849875483379879503746586379571948616648440931782652980425872757672991175590086646394353887802616879562644198824690327989301513611998764748003811531167214679540635804391942069573888163886407070760389175334936581555E-1029M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))

(prn (let [x (read-string "-6784325061858374798615545734827229586433692271286209510728266035607858608277366198740932853927042853948862032829815535710756625035065329708491921738865913890260989698108821310596099455147176953019923422348730679065428851037800849270547108511121248707185002403578355982679801575791994563195898932274805339948279833627022141883341299596410769543028163225689425273474490142560282137071525171497888083173015664630372923266915966653706541328243275859256986059390729305695659315020921915255407074869219980418217967722821817628304929281330813026821838560492455438109777136528930145917093617069866220166858365530531758581593884074962713021742495946027380566096663073172208810108062542839018708590656207600538488005680806061159817117937446423635181161090135903445427193662453949287670168906667040504057635833572325321609849875483379879503746586379571948616648440931782652980425872757672991175590086646394353887802616879562644198824690327989301513611998764748003811531167214679540635804391942069573888163886407070760389175334936581555E-1030M")] [(str x) (pr-str x) (decimal? x) (= (class x) BigDecimal)]))
