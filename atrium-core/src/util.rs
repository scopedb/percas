pub fn num_cpus() -> usize {
    std::thread::available_parallelism().unwrap().get().max(1)
}
