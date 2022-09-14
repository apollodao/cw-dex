pub(crate) fn vec_into<A, B: Into<A>>(v: Vec<B>) -> Vec<A> {
    v.into_iter().map(|x| x.into()).collect()
}
