// Copyright 2017 PingCAP, Inc.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// See the License for the specific language governing permissions and
// limitations under the License.


#[macro_use]
extern crate futures;
extern crate grpc;
extern crate grpc_proto;
extern crate protobuf;
extern crate rand;
extern crate serde;
extern crate serde_json;
#[macro_use]
extern crate serde_derive;

mod util;

use std::sync::Arc;
use std::time::Duration;
use std::thread;

use futures::{Future, Sink, Stream, future};
use grpc::{ChannelBuilder, Environment, Error};
use grpc_proto::example::route_guide::{Point, Rectangle, RouteNote};
use grpc_proto::example::route_guide_grpc::RouteGuideClient;
use rand::Rng;

fn new_point(lat: i32, lon: i32) -> Point {
    let mut point = Point::new();
    point.set_latitude(lat);
    point.set_longitude(lon);
    point
}

fn new_rect(lat1: i32, lon1: i32, lat2: i32, lon2: i32) -> Rectangle {
    let mut rect = Rectangle::new();
    rect.set_lo(new_point(lat1, lon1));
    rect.set_hi(new_point(lat2, lon2));
    rect
}

fn new_note(lat: i32, lon: i32, msg: &str) -> RouteNote {
    let mut note = RouteNote::new();
    note.set_location(new_point(lat, lon));
    note.set_message(msg.to_owned());
    note
}

fn get_feature(client: &RouteGuideClient, point: Point) {
    let get_feature = client.get_feature_async(point.clone());
    match get_feature.wait() {
        Err(e) => panic!("RPC failed: {:?}", e),
        Ok(f) => {
            if !f.has_location() {
                println!("Server returns incomplete feature.");
                return;
            }
            if f.get_name().is_empty() {
                println!("Found no feature at {}", util::format_point(&point));
                return;
            }
            println!("Found feature called {} at {}",
                     f.get_name(),
                     util::format_point(&point));
        }
    }
}

fn list_features(client: &RouteGuideClient) {
    let rect = new_rect(400000000, -750000000, 420000000, -730000000);
    println!("Looking for features between 40, -75 and 42, -73");
    let mut list_features = client.list_features(rect);
    loop {
        let f = list_features.into_future();
        match f.wait() {
            Ok((Some(feature), s)) => {
                list_features = s;
                let loc = feature.get_location();
                println!("Found feature {} at {}",
                         feature.get_name(),
                         util::format_point(&loc));
            }
            Ok((None, _)) => break,
            Err((e, _)) => panic!("List features failed: {:?}", e),
        }
    }
    println!("List feature rpc succeeded.");
}

fn record_route(client: &RouteGuideClient) {
    let features = util::load_db();
    let mut rng = rand::thread_rng();
    let mut call = client.record_route();
    for _ in 0..10 {
        let f = rng.choose(&features).unwrap();
        let point = f.get_location();
        println!("Visiting {}", util::format_point(point));
        call = call.send(point.to_owned()).wait().unwrap();
        thread::sleep(Duration::from_millis(rng.gen_range(500, 1500)));
    }
    let (mut call, mut receiver) = (Some(call), None);
    let sumary = future::poll_fn::<_, Error, _>(|| {
        if let Some(ref mut c) = call {
            try_ready!(c.close());
        }
        if call.is_some() {
            receiver = Some(call.take().unwrap().into_receiver());
        }
        receiver.as_mut().unwrap().poll()
    })
            .wait()
            .unwrap();
    println!("Finished trip with {} points", sumary.get_point_count());
    println!("Passed {} features", sumary.get_feature_count());
    println!("Travelled {} meters", sumary.get_distance());
    println!("It took {} seconds", sumary.get_elapsed_time());
}

fn route_chat(client: &RouteGuideClient) {
    let mut call = client.route_chat();
    let mut receiver = call.take_receiver().unwrap();
    let h = thread::spawn(move || {
        let notes = vec![
            ("First message", 0, 0),
            ("Second message", 0, 1),
            ("Third message", 1, 0),
            ("Fourth message", 0, 0),
        ];

        for (msg, lat, lon) in notes {
            let note = new_note(lat, lon, msg);
            println!("Sending message {} at {}, {}", msg, lat, lon);
            call = call.send(note).wait().unwrap();
        }
        future::poll_fn(|| call.close()).wait().unwrap();
    });

    loop {
        match receiver.into_future().wait() {
            Ok((Some(note), r)) => {
                let location = note.get_location();
                println!("Got message {} at {}, {}",
                         note.get_message(),
                         location.get_latitude(),
                         location.get_longitude());
                receiver = r;
            }
            Ok((None, _)) => break,
            Err((e, _)) => panic!("RouteChat RPC failed: {:?}", e),
        }
    }

    h.join().unwrap();
}

fn main() {
    let env = Arc::new(Environment::new(2));
    let channel = ChannelBuilder::new(env).connect("127.0.0.1:50051");
    let client = RouteGuideClient::new(channel);

    println!("-------------- GetFeature --------------");
    get_feature(&client, new_point(409146138, -746188906));
    get_feature(&client, new_point(0, 0));

    println!("-------------- ListFeatures --------------");
    list_features(&client);

    println!("-------------- RecordRoute --------------");
    record_route(&client);

    println!("-------------- RouteChat --------------");
    route_chat(&client);
}