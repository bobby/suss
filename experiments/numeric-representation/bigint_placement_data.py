# Original, explicit operand geometry shared by validation and measurement.
SHAPES = [(n,n) for n in [1,2,3,4,8,32,128,512]] + [
    (1,8),(8,1),(1,128),(128,1),(1,512),(512,1),
    (2,512),(512,2),(32,128),(128,32),
]
