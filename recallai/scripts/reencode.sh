DIR=output
VID=$DIR/output.rvid
JPG="$DIR/output-jpgs"
IN=input/video.rvid
RULES=input/rules.json

#/usr/bin/time -v ./target/release/recallai-vid-transcode -i input/video.rvid -r input/rules.json --timing --disable-parallel -o $VID
#/usr/bin/time -v ./target/release/recallai-vid-transcode -i input/video.rvid -r input/rules.json --timing --mp-mc -o $VID
#/usr/bin/time -v ./target/release/recallai-vid-transcode -i input/video.rvid -r input/rules.json --timing --num-threads 8 --mp-mc -o $VID
#/usr/bin/time -v ./target/release/recallai-vid-transcode -i input/video.rvid -r input/rules.json --timing --num-threads 5 --mp-mc -o $VID
#/usr/bin/time -v ./target/release/recallai-vid-transcode -i input/video.rvid -r input/rules.json --timing --num-threads 4 --mp-mc -o $VID
#/usr/bin/time -v ./target/release/recallai-vid-transcode -i input/video.rvid -r input/rules.json --timing --num-threads 3 --mp-mc -o $VID
#/usr/bin/time -v ./target/release/recallai-vid-transcode -i input/video.rvid -r input/rules.json --timing --num-threads 2 --mp-mc -o $VID
#/usr/bin/time -v ./target/release/recallai-vid-transcode -i input/video.rvid -r input/rules.json --timing --num-threads 1 --mp-mc -o $VID
#/usr/bin/time -v ./target/release/recallai-vid-transcode -i input/video.rvid -r input/rules.json --timing -o $VID
#/usr/bin/time -v ./target/release/recallai-vid-transcode -i input/video.rvid -r input/rules.json --timing --num-threads 1 -o $VID
#/usr/bin/time -v ./target/release/recallai-vid-transcode -i input/video.rvid -r input/rules.json --timing --num-threads 2 -o $VID
#/usr/bin/time -v ./target/release/recallai-vid-transcode -i input/video.rvid -r input/rules.json --timing --num-threads 3 -o $VID
#/usr/bin/time -v ./target/release/recallai-vid-transcode -i input/video.rvid -r input/rules.json --timing --num-threads 4 -o $VID
#/usr/bin/time -v ./target/release/recallai-vid-transcode -i input/video.rvid -r input/rules.json --timing --num-threads 5 -o $VID
#

#/usr/bin/time -v ./target/release/recallai-vid-transcode -i input/video.rvid -r input/rules.json --timing --num-threads 4 --disable-parallel --debug -o $VID

#/usr/bin/time -v ./target/release/recallai-vid-transcode -i input/video.rvid -r input/rules.json --timing --glommio -o $VID
#/usr/bin/time -v ./target/release/recallai-vid-transcode -i input/video.rvid -r input/rules.json --timing --glommio --num-threads 1 -o $VID
#/usr/bin/time -v ./target/release/recallai-vid-transcode -i input/video.rvid -r input/rules.json --timing --disable-parallel -o $VID
#/usr/bin/time -v ./target/release/recallai-vid-transcode -i input/video.rvid -r input/rules.json --timing -o $VID
#/usr/bin/time -v ./target/release/recallai-vid-transcode -i input/video.rvid -r input/rules.json --timing --mp-mc --num-threads 6 -o $VID
#/usr/bin/time -v ./target/release/recallai-vid-transcode -i input/video.rvid -r input/rules.json --timing --num-threads 6 -o $VID
#/usr/bin/time -v ./target/release/recallai-vid-transcode -i input/video.rvid -r input/rules.json --timing --disable-parallel --num-threads 6 -o $VID
#/usr/bin/time -v ./target/release/recallai-vid-transcode -i input/video.rvid -r input/rules.json --timing --num-threads 6 --glommio -o $VID
#/usr/bin/time -v ./target/release/recallai-vid-transcode -i input/video.rvid -r input/rules.json --timing --num-threads 6 --tokio -o $VID
#/usr/bin/time -v ./target/release/recallai-vid-transcode -i $IN -r $RULES --timing --num-threads 5 --tokio -o $VID
/usr/bin/time -v ./target/release/recallai-vid-transcode -i $IN -r $RULES --timing --num-threads 6 -o $VID
DIR=output-test
VID=$DIR/output-small.rvid
JPG="$DIR/output-small-jpgs"
IN=input-test/input.rvid
RULES=input-test/rules.json

#/usr/bin/time -v ./target/release/recallai-vid-transcode -i $IN -r $RULES --timing --num-threads 6 --tokio -o $VID
