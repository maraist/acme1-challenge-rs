#!/bin/bash
OK=./full.json
BAD=./partial.json
USE=$OK
if [[ $1 == "bad" ]]; then
  USE=$BAD
fi

#curl -v -X POST -d @$USE -H "Content-Type: application/json" http://localhost:3000/recompose

#curl -X POST -d @samp.rvid  'http://localhost:3000/crc'
#curl -X POST -d @samp.rvid  'http://localhost:3000/crc'
curl -v -X POST -H "Content-Type: application/json" -d @reflect.json 'http://34.229.172.229:80/recompose'
