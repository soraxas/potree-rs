use potree::{point_cloud::PointCloudAsync, prelude::*};
use tracing::info;
use tracing_subscriber::fmt;
use tracing_subscriber_wasm::MakeConsoleWriter;
use wasm_bindgen_futures::spawn_local;
use wasm_thread as thread;

pub fn main() {
    fmt()
        .with_writer(
            // To avoide trace events in the browser from showing their
            // JS backtrace, which is very annoying, in my opinion
            MakeConsoleWriter::default().map_trace_level_to(tracing::Level::DEBUG),
        )
        // For some reason, if we don't do this in the browser, we get
        // a runtime error.
        .without_time()
        .init();

    spawn_local(async move {
        info!("Load pointcloud from local filesystem");
        // must be instantiated in main thread so the http requests are made on the main thread
        let mut point_cloud = PointCloud::from_http_url("http://localhost:8080/assets/heidentor/")
            .await
            .expect("Unable to load point cloud");

        thread::spawn({
            // let resource_loader = resource_loader.clone();
            || {
                info!("Hello from thread!");

                spawn_local(async move {
                    info!("Hello from spawned local!");

                    tracing::info!("Successfuly loaded point cloud hierarchy.");
                    let snapshot = point_cloud.hierarchy_snapshot();
                    tracing::info!(
                        "Successfuly loaded point cloud hierarchy with {} nodes",
                        snapshot.len()
                    );

                    let points = point_cloud
                        .load_points(point_cloud.octree().root_id())
                        .await
                        .expect("Unable to load points");

                    tracing::info!("Loaded {} points", points.points.len());

                    point_cloud
                        .load_entire_hierarchy()
                        .await
                        .expect("Unable to load entire hierarchy");

                    let full_snapshot = point_cloud.hierarchy_snapshot();
                    tracing::info!(
                        "Successfuly loaded entire point cloud hierarchy with {} nodes.",
                        full_snapshot.len()
                    );
                });

                wasm_bindgen::throw_str(
                "Cursed hack to keep workers alive. See https://github.com/rustwasm/wasm-bindgen/issues/2945",
            );
            }
        });
    });
}
