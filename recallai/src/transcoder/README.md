# transcoder

This module represents a polymorphic transcoder-dispatch. fn-main picks which library to activate for the main execution

# strategy-makeup

Since different strategies require different inputs and outputs, we use an adapter to match an enumerated input/output
to a best-fit IO strategy

# strategies

* spstsc - Single Producer (read), Single Transformer, Single Consumer (write). e.g. a simple read, modify, write loop. useful as a reference implementation. Uses std Rust.
* spmtsc - Single Producer (read), MultiThreaded Transformer, Single Consumer (write). Requires an out of order buffer manager. Leverages crossbeam for a spmc , but std Rust for scoped threads.
* mpmtmp - Multi Producer (read), Multi Transformer, Multi Consumer (write). Requires random-access read for producer, and Random-Access Write for consumer. Uses std Rust. Uses a thread-per read-modify-write worker.
* glommio - io_uring based event loop.  Leverages DMA (direct from disk, to disk). Leverages low-latency peer-thread message passing. To facilitate a fixed buffer workflow with rust's borrow-checker, utilizes a dual-read IoSlice.
* tokioworker - Uses tokio-async IO, locks, channels, blocking-tasks. The goal is complete non blocking workflow, with a pool of blocking transcoders.

# assessment

Tested on a 10core 20thread machine (i9-10850), and on a c7i.2xlarge (4 core 8 thread).

spmtsc at 4 concurrent workers (transcoders) on a 10 core machine seems to have the best performance-vs-memory tradeoff.
spmtsc at 6 concurrent workers (transcoders) on a 10 core machine seems to reach the asymptote of max performance. Going higher does increase throughput but by under 1%.

mpmtmp doesn't do as well as spmtsc at any level, and consumes a LOT more RAM (since it uses more buffers).

glommio runs slower - but this is expected because it is throttled by the DMA actual transfer. In the other benchmarks, several GB of output data is still not yet written by the time the program exits.. Larger-than-RAM tests would need to be performed to validate if this sustains a faster average throughput.  The c7i instance, for example only had 128MB/s of disk-IO throughput and thus was many times slower than a cached-read, cached-write.

tokio ran very well - almost identically to spmtsc, and only using a few percent more RAM. I was surprised since it is doing a lot more work per task. The unused CPU cycles while waiting on IO most likely allowed that extra innefficiency to not amount to any slowdowns. So this seems like a perfectly viable solution when ALREADY using tokio - say for a web-server or AWS lambda infrastructure.