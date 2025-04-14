use behavior_tests::Testkit;
use behavior_tests::harness;
use behavior_tests::render_hex;
use insta::assert_snapshot;
use test_harness::test;

#[test(harness)]
async fn test_put_get(testkit: Testkit) {
    testkit.client.put("key", "value".as_bytes()).await.unwrap();
    let value = testkit.client.get("key").await.unwrap();
    assert_snapshot!(
        render_hex(value.unwrap()),
        @r"
        Length: 5 (0x5) bytes
        0000:   76 61 6c 75 65            value
        "
    );
}
