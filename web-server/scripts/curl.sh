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
curl -v -X POST -H "Content-Type: application/json" -d @reflect2.json 'http://localhost:3000/recompose'
curl -v -X POST -H "Content-Type: application/json" -d @reflect_big.json 'http://localhost:3000/recompose'
#curl -v -X POST -H "Content-Type: application/json" -d @reflect.json 'http://localhost:8080/recompose'
#curl -v -X POST -H "Content-Type: application/json" -d @reflect1.json 'http://localhost:8080/recompose'
