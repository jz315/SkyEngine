pub fn sample_entities<T: Copy>(entities: &[T], count: usize) -> Vec<T> {
    assert!(count > 0);
    assert!(entities.len() >= count);

    let mut sampled: Vec<T> = (0..count)
        .map(|index| entities[index * entities.len() / count])
        .collect();
    crate::common::deterministic_shuffle(&mut sampled);
    sampled
}
