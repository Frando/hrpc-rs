const RPC = require('arpeecee')
const net = require('net')
const messages = require('./messages')

class Session {
  constructor (socket) {
    this.rpc = new RPC()
    this.rpc.on('error', err => {
      console.error('rpc error: ' + err.message)
    })
    this.shoutService = this.rpc.defineService({ id: 1 })
    this.shout = this.shoutService
      .defineMethod({
        id: 1,
        requestEncoding: messages.ShoutRequest,
        responseEncoding: messages.ShoutResponse,
        onrequest ({ message }) {
          console.log('hi!', message)
          return { message: message.toUpperCase(), loudness: 10 }
          // return Buffer.from(val.toString().toUpperCase())
        }
      })
    this.pad = this.shoutService
      .defineMethod({
        id: 2,
        requestEncoding: RPC.BINARY,
        responseEncoding: RPC.BINARY,
        // onrequest (val) {
        //   console.log('hi!', val)
        //   return Buffer.from('~~' + val.toString() + '~~')
        // }
      })
    socket.pipe(this.rpc).pipe(socket)
    socket.on('data', d => console.log('DATA', d))
  }
}

const mode = process.argv[2]
const port = 8080
if (mode === 'client') client()
else if (mode === 'server') server()
else console.error('usage: node basic.js <client|server>')

function client () {
  console.log('now connect')
  const socket = net.connect(port, onconnection)
  function onconnection () {
    console.log('onconnection')
    const session = new Session(socket)
    for (let i = 0; i < 2; i++) {
      session.shout.request({ message: 'hello' + i })
        .then(res => console.log('shout res', res))
        .catch(err => console.log('shout ERR', err))
    }
  }
  // session.pad.request('world')
  //   .then(b => b.toString()).then('pad res', console.log)
  //   .catch('pad err', console.log)
}
function server () {
  const server = net.createServer(onconnection)
  server.on('error', err => {
    console.error('error: ' + err.message)
  })
  server.listen(port)
  function onconnection (socket) {
    const session = new Session(socket)
  }
}
