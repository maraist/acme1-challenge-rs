// A simple reusable pool of fixed-length buffers.
// TODO Note, no guards are employed to guarantee the vector retains the exact size. should assert
pub(crate) struct BufStack {
    stack: Vec<Vec<u8>>,
    sz: usize,
}

impl BufStack {
    pub(crate) fn new(sz: usize) -> Self {
        BufStack { stack: vec![], sz }
    }
    pub(crate) fn pop(&mut self) -> Vec<u8> {
        if let Some(b) = self.stack.pop() {
            b
        } else {
            vec![0u8; self.sz]
        }
    }

    // TODO assert buf-len is unchanged
    #[allow(dead_code)]
    pub(crate) fn push(&mut self, buf: Vec<u8>) {
        self.stack.push(buf);
    }
}
