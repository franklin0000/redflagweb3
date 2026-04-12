import { useState, useEffect, useCallback, useRef } from 'react';
import {
  Activity, Wallet, Database, Send, ShieldCheck, Zap,
  Globe, Lock, Users, FileCode, Copy, RefreshCw, Plus,
  Search, Settings, LogOut, CheckCircle2, AlertTriangle,
  QrCode, TrendingUp, Eye, EyeOff, Download, BarChart3,
  ArrowUpRight, ArrowDownLeft, Clock, Shield, Cpu, Key,
  ArrowLeftRight, Droplets, TrendingDown, ChevronDown
} from 'lucide-react';
import { AreaChart, Area, LineChart, Line, XAxis, YAxis, Tooltip, ResponsiveContainer, CartesianGrid } from 'recharts';
import { QRCodeSVG } from 'qrcode.react';
import sdk from './sdk';
import { generateMnemonic, validateMnemonic } from './mnemonic';
import { encryptWallet, decryptWallet, loadKeystore, saveKeystore, deleteKeystore, hasKeystore } from './crypto';

// ─────────────────────────────────────────────
// UTILS
// ─────────────────────────────────────────────
const copy = t => navigator.clipboard?.writeText(t).catch(()=>{});
const short = (s,n=14) => s && s.length > n*2+3 ? `${s.slice(0,n)}…${s.slice(-6)}` : (s||'—');
const fmtN = n => typeof n === 'number' ? n.toLocaleString() : (n||0);
const fmtDate = ts => ts ? new Date(ts*1000).toLocaleString() : '—';
const fmtRF = n => `${fmtN(n)} RF`;

// ─────────────────────────────────────────────
// MICRO COMPONENTS
// ─────────────────────────────────────────────
const Dot = ({on}) => <span className={`ldot ${on?'on':'off'}`}/>;

const Alert = ({type='ok',children}) => (
  <div className={`alt alt-${type}`}>
    {type==='ok'  && <CheckCircle2 size={15}/>}
    {type==='err' && <AlertTriangle size={15}/>}
    {type==='inf' && <ShieldCheck size={15}/>}
    {type==='wrn' && <AlertTriangle size={15}/>}
    <span>{children}</span>
  </div>
);

const Chip = ({color='r',children}) => <span className={`bdg bdg-${color}`}>{children}</span>;

const CopyBtn = ({text}) => {
  const [done,setDone] = useState(false);
  const handle = () => { copy(text); setDone(true); setTimeout(()=>setDone(false),1500); };
  return <button className="cpbtn" onClick={handle} title="Copiar">{done ? <CheckCircle2 size={13}/> : <Copy size={13}/>}</button>;
};

const StatCard = ({icon:Icon,label,value,sub,color='var(--red)'}) => (
  <div className="card fi" style={{display:'flex',flexDirection:'column',gap:4}}>
    <div style={{display:'flex',justifyContent:'space-between',alignItems:'center'}}>
      <div className="stat-ico" style={{background:`${color}14`}}><Icon size={18} color={color}/></div>
      <span style={{fontSize:10,color:'var(--txl)',fontWeight:700,letterSpacing:1}}>LIVE</span>
    </div>
    <div className="stat-val">{value}</div>
    <div className="stat-lbl">{label}</div>
    {sub && <div className="stat-sub" style={{color}}>{sub}</div>}
  </div>
);

const SectionHdr = ({icon:Icon,title,right,color='var(--red)'}) => (
  <div className="sec-hdr">
    <div className="sec-ttl"><Icon size={16} color={color}/>{title}</div>
    {right}
  </div>
);

// ─────────────────────────────────────────────
// LOCK / ONBOARDING
// ─────────────────────────────────────────────
function LockScreen({onUnlock}) {
  const [pw,setPw] = useState('');
  const [err,setErr] = useState('');
  const [loading,setLoading] = useState(false);
  const handle = async(e)=>{
    e.preventDefault(); setErr(''); setLoading(true);
    try {
      const ks = loadKeystore();
      const data = await decryptWallet(ks,pw);
      onUnlock(data);
    } catch(ex){ setErr(ex.message); }
    finally { setLoading(false); }
  };
  return (
    <div className="lock-sc">
      <div className="card lock-card fi">
        <div style={{textAlign:'center',marginBottom:32}}>
          <img src="./logo.png" alt="RedFlag" style={{width:72,height:72,borderRadius:12,objectFit:'contain',margin:'0 auto 14px',display:'block'}}/>
          <h1 style={{fontFamily:'var(--acc)',fontSize:24,marginBottom:6}}>redflag.web3</h1>
          <p style={{color:'var(--txm)',fontSize:13}}>Enter your password to unlock</p>
        </div>
        {err && <Alert type="err">{err}</Alert>}
        <form onSubmit={handle}>
          <div className="field">
            <label className="inp-lbl">Password</label>
            <input type="password" className="inp" value={pw} onChange={e=>setPw(e.target.value)} autoFocus placeholder="••••••••" required/>
          </div>
          <button type="submit" className="btn btn-p btn-fw btn-lg" disabled={loading||!pw}>
            {loading ? <span className="spin"><RefreshCw size={14}/></span> : <><Lock size={15}/> Unlock</>}
          </button>
        </form>
      </div>
    </div>
  );
}

function Onboarding({onComplete}) {
  const [step,setStep] = useState('welcome'); // welcome|create|verify|import|password
  const [mn,setMn] = useState('');
  const [verify,setVerify] = useState('');
  const [pw,setPw] = useState('');
  const [pw2,setPw2] = useState('');
  const [impMn,setImpMn] = useState('');
  const [loading,setLoading] = useState(false);
  const [err,setErr] = useState('');
  const steps = {welcome:0,create:1,verify:2,password:3};

  const startCreate = () => { setMn(generateMnemonic()); setStep('create'); };
  
  const finish = async() => {
    setErr(''); setLoading(true);
    try {
      if(pw !== pw2) throw new Error('Passwords do not match');
      if(pw.length < 8)  throw new Error('Password must be at least 8 characters');
      const mnemonic = step === 'password' && impMn ? impMn : mn;
      const res = await sdk.walletNew();
      const data = { address: res.address, private_key_hex: res.private_key_hex, mnemonic, created: Date.now() };
      const ks = await encryptWallet(data, pw);
      saveKeystore(ks);
      onComplete(data);
    } catch(ex){ setErr(ex.message); }
    finally { setLoading(false); }
  };

  return (
    <div className="lock-sc">
      <div className="card lock-card fi">
        <div className="step-dots">
          {Object.values(steps).map(i => <span key={i} className={`step-dot ${(steps[step]??0)>=i?'on':''}`}/>)}
        </div>

        {step==='welcome' && (
          <div style={{textAlign:'center'}}>
            <ShieldCheck size={56} color="var(--red)" style={{margin:'0 auto 16px'}}/>
            <h2 style={{fontFamily:'var(--acc)',fontSize:24,marginBottom:10}}>Quantum Wallet</h2>
            <p style={{color:'var(--txm)',fontSize:13,lineHeight:1.7,marginBottom:32}}>
              Post-quantum security with ML-DSA-65 (FIPS 204).<br/>
              Your keys are resistant to quantum computer attacks.
            </p>
            <button className="btn btn-p btn-fw btn-lg" style={{marginBottom:12}} onClick={startCreate}>
              <Plus size={16}/> Create New Wallet
            </button>
            <button className="btn btn-o btn-fw" onClick={()=>setStep('import')}>
              <Download size={15}/> Import with Seed Phrase
            </button>
          </div>
        )}

        {step==='create' && (
          <div>
            <h3 style={{fontFamily:'var(--acc)',marginBottom:6}}>Recovery Phrase</h3>
            <p style={{color:'var(--txm)',fontSize:12,marginBottom:16}}>Write these 12 words down and store them safely. Never share them.</p>
            <div className="mn-grid" style={{marginBottom:20}}>
              {mn.split(' ').map((w,i)=>(
                <div key={i} className="mn-word">
                  <span className="mn-i">{i+1}.</span>
                  <span className="mn-w">{w}</span>
                </div>
              ))}
            </div>
            <div className="alt alt-wrn" style={{fontSize:12,marginBottom:16}}>
              <AlertTriangle size={14}/> Store this phrase offline. Anyone with it controls your funds.
            </div>
            <button className="btn btn-p btn-fw" onClick={()=>setStep('password')}>I've saved it securely →</button>
          </div>
        )}

        {step==='import' && (
          <div>
            <h3 style={{fontFamily:'var(--acc)',marginBottom:12}}>Import Wallet</h3>
            <div className="field">
              <label className="inp-lbl">12-word seed phrase</label>
              <textarea className="inp mono" rows={4} placeholder="word1 word2 word3 ..." value={impMn} onChange={e=>setImpMn(e.target.value)} style={{resize:'none'}}/>
            </div>
            {err && <Alert type="err">{err}</Alert>}
            <button className="btn btn-p btn-fw" disabled={!validateMnemonic(impMn.trim())} onClick={()=>{ setMn(impMn.trim()); setStep('password'); }}>Continue →</button>
            <button className="btn btn-o btn-fw" style={{marginTop:8}} onClick={()=>setStep('welcome')}>← Back</button>
          </div>
        )}

        {step==='password' && (
          <div>
            <h3 style={{fontFamily:'var(--acc)',marginBottom:12}}>Set Encryption Password</h3>
            <p style={{color:'var(--txm)',fontSize:12,marginBottom:16}}>AES-256-GCM + PBKDF2 (200K iterations). Your private key is never stored in plaintext.</p>
            <div className="field">
              <label className="inp-lbl">Password</label>
              <input type="password" className="inp" value={pw} onChange={e=>setPw(e.target.value)} placeholder="min 8 characters"/>
            </div>
            <div className="field">
              <label className="inp-lbl">Confirm Password</label>
              <input type="password" className="inp" value={pw2} onChange={e=>setPw2(e.target.value)} placeholder="repeat password"/>
            </div>
            {pw && (
              <div className="pbr" style={{marginBottom:12}}>
                <div className="pfill" style={{width: pw.length>=12?'100%':pw.length>=8?'66%':'33%'}}/>
              </div>
            )}
            {err && <Alert type="err">{err}</Alert>}
            <button className="btn btn-p btn-fw" disabled={!pw||!pw2||loading} onClick={finish}>
              {loading ? <><span className="spin"><RefreshCw size={14}/></span> Encrypting…</> : <><Lock size={14}/> Create Wallet</>}
            </button>
          </div>
        )}
      </div>
    </div>
  );
}

// ─────────────────────────────────────────────
// WALLET PAGE
// ─────────────────────────────────────────────
function WalletPage({wallet,wsData}) {
  const [account,setAccount] = useState({balance:0,nonce:0,exists:false});
  const [history,setHistory] = useState([]);
  const [tab,setTab] = useState('send');
  const [result,setResult] = useState(null);
  const [loading,setLoading] = useState(false);
  const [showKey,setShowKey] = useState(false);
  const [showQR,setShowQR] = useState(false);
  const [faucetAmt,setFaucetAmt] = useState('1000');
  const [send,setSend] = useState({to:'',amount:'',fee:'1'});

  const refresh = useCallback(async()=>{
    if(!wallet?.address) return;
    try {
      const [acc,hist] = await Promise.all([sdk.getAccount(wallet.address), sdk.getHistory(wallet.address)]);
      if(acc) setAccount(acc);
      setHistory(hist||[]);
    } catch{}
  },[wallet?.address]);

  useEffect(()=>{ refresh(); },[refresh]);
  useEffect(()=>{ if(wsData?.type==='new_block'||wsData?.type==='faucet') refresh(); },[wsData,refresh]);

  const handleFaucet = async()=>{
    setLoading(true); setResult(null);
    try {
      const r = await sdk.walletFaucet(wallet.address, parseInt(faucetAmt)||1000);
      setResult({ok:r.accepted, msg:r.message, hash:r.tx_hash});
    } catch(e){ setResult({ok:false,msg:e.response?.data?.message||e.message}); }
    finally { setLoading(false); }
  };

  const handleSend = async(e)=>{
    e.preventDefault(); setLoading(true); setResult(null);
    try {
      const r = await sdk.walletSend(wallet.private_key_hex, send.to, parseInt(send.amount), parseInt(send.fee)||1);
      setResult({ok:r.accepted, msg:r.message, hash:r.tx_hash});
      if(r.accepted){ setSend({to:'',amount:'',fee:'1'}); setTimeout(refresh,2000); }
    } catch(e){ setResult({ok:false, msg:e.response?.data?.message||e.message}); }
    finally { setLoading(false); }
  };

  // Sparkline from history (last 10 balances)
  const sparkData = history.slice(0,10).reverse().map((tx,i)=>({
    i, v: tx.receiver===wallet.address ? tx.amount : -tx.amount
  }));

  return (
    <div className="pg">
      {/* Balance card */}
      <div className="c5">
        <div className="card cr fi" style={{height:'100%'}}>
          <div style={{display:'flex',justifyContent:'space-between',alignItems:'flex-start',marginBottom:20}}>
            <Chip color="r">ML-DSA-65</Chip>
            <div style={{display:'flex',gap:6}}>
              <button className="ibtn btn-sm" onClick={()=>setShowQR(!showQR)} title="QR"><QrCode size={14}/></button>
              <button className="ibtn btn-sm" onClick={refresh} title="Refresh"><RefreshCw size={14}/></button>
            </div>
          </div>

          {showQR && <div style={{marginBottom:20}}><div className="qr-wrap"><QRCodeSVG value={wallet.address||''} size={140}/></div></div>}

          <div className="bal-big">{fmtN(account.balance)}<span className="bal-unit">RF</span></div>
          <div style={{marginTop:8,fontSize:12,color:'var(--txm)'}}>Nonce: {account.nonce} · {account.exists?<Chip color="g">Active</Chip>:<Chip color="x">No funds yet</Chip>}</div>

          <div className="dvd"/>

          <div style={{marginBottom:8}}>
            <div style={{fontSize:10,color:'var(--txl)',fontWeight:700,letterSpacing:.7,marginBottom:6}}>ADDRESS</div>
            <div style={{display:'flex',alignItems:'center',gap:8}}>
              <span style={{fontFamily:'var(--mono)',fontSize:10.5,color:'var(--cyan)',wordBreak:'break-all',flex:1}}>{short(wallet.address,18)}</span>
              <CopyBtn text={wallet.address}/>
            </div>
          </div>

          <details style={{marginTop:14}}>
            <summary style={{cursor:'pointer',fontSize:11,color:'var(--txl)',userSelect:'none',display:'flex',alignItems:'center',gap:6}}>
              <Key size={12}/> Private Key (keep secret)
            </summary>
            <div style={{marginTop:10}}>
              <div className="key-blk">{showKey ? short(wallet.private_key_hex,32) : '••••••••••••••••••••••••••••••••'}</div>
              <div style={{display:'flex',gap:8,marginTop:8}}>
                <button className="ibtn btn-sm" onClick={()=>setShowKey(!showKey)}>{showKey?<EyeOff size={13}/>:<Eye size={13}/>}</button>
                <CopyBtn text={wallet.private_key_hex}/>
              </div>
            </div>
          </details>
        </div>
      </div>

      {/* Actions panel */}
      <div className="c7">
        <div className="card fi" style={{height:'100%'}}>
          <div className="tabs">
            {[['send','Send'],['faucet','Faucet'],['history','History']].map(([id,lbl])=>(
              <button key={id} className={`tab ${tab===id?'on':''}`} onClick={()=>setTab(id)}>{lbl}</button>
            ))}
          </div>

          {result && <Alert type={result.ok?'ok':'err'}>{result.msg}{result.hash&&<><br/><span style={{fontFamily:'var(--mono)',fontSize:10}}>tx: {result.hash.slice(0,32)}…</span></>}</Alert>}

          {tab==='send' && (
            <form onSubmit={handleSend}>
              <div className="field">
                <label className="inp-lbl">To Address</label>
                <input className="inp mono" placeholder="hex address…" value={send.to} onChange={e=>setSend({...send,to:e.target.value})} required/>
              </div>
              <div style={{display:'grid',gridTemplateColumns:'1fr 100px',gap:12}}>
                <div className="field">
                  <label className="inp-lbl">Amount (RF)</label>
                  <input type="number" className="inp" min="1" placeholder="100" value={send.amount} onChange={e=>setSend({...send,amount:e.target.value})} required/>
                </div>
                <div className="field">
                  <label className="inp-lbl">Fee</label>
                  <input type="number" className="inp" min="1" value={send.fee} onChange={e=>setSend({...send,fee:e.target.value})}/>
                </div>
              </div>
              <div style={{display:'flex',justifyContent:'space-between',alignItems:'center',marginBottom:16,fontSize:12,color:'var(--txm)'}}>
                <span>Available: <b style={{color:'var(--txt)'}}>{fmtRF(account.balance)}</b></span>
                <span>Nonce: {account.nonce}</span>
              </div>
              <button type="submit" className="btn btn-p btn-fw" disabled={loading||!send.to||!send.amount}>
                {loading ? <><span className="spin"><RefreshCw size={14}/></span> Signing…</> : <><Send size={14}/> Sign & Broadcast</>}
              </button>
            </form>
          )}

          {tab==='faucet' && (
            <div>
              <div className="alt alt-inf" style={{marginBottom:20}}>
                <ShieldCheck size={14}/>Free testnet tokens (max 10,000 RF per request)
              </div>
              <div className="field">
                <label className="inp-lbl">Amount (max 10,000)</label>
                <input type="number" className="inp" min="1" max="10000" value={faucetAmt} onChange={e=>setFaucetAmt(e.target.value)} placeholder="1000"/>
              </div>
              <button className="btn btn-gr btn-fw" onClick={handleFaucet} disabled={loading}>
                {loading ? <><span className="spin"><RefreshCw size={14}/></span> Requesting…</> : '💧 Request RF Tokens'}
              </button>
            </div>
          )}

          {tab==='history' && (
            <div style={{maxHeight:360,overflowY:'auto'}}>
              {history.length===0 ? (
                <div style={{textAlign:'center',padding:'40px 0',color:'var(--txl)'}}>
                  <Clock size={32} style={{margin:'0 auto 10px',display:'block'}}/>No transactions yet
                </div>
              ) : (
                <table className="tbl">
                  <thead><tr><th>Type</th><th>Party</th><th>Amount</th><th>Fee</th><th>Date</th></tr></thead>
                  <tbody>
                    {history.map((tx,i)=>{
                      const isSend = tx.sender===wallet.address;
                      return <tr key={i}>
                        <td><span style={{color:isSend?'#ff6b6b':'var(--green)',display:'flex',alignItems:'center',gap:4,fontWeight:700,fontSize:12}}>
                          {isSend?<ArrowUpRight size={13}/>:<ArrowDownLeft size={13}/>}{isSend?'SEND':'RECV'}
                        </span></td>
                        <td className="mono">{short(isSend?tx.receiver:tx.sender,12)}</td>
                        <td style={{color:isSend?'#ff6b6b':'var(--green)',fontWeight:600}}>{isSend?'-':'+'}{fmtN(tx.amount)} RF</td>
                        <td style={{color:'var(--txl)'}}>{tx.fee} RF</td>
                        <td style={{color:'var(--txl)',fontSize:11}}>{fmtDate(tx.timestamp)}</td>
                      </tr>;
                    })}
                  </tbody>
                </table>
              )}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

// ─────────────────────────────────────────────
// DASHBOARD PAGE
// ─────────────────────────────────────────────
function DashboardPage({stats,vertices,mempool,roundEk,tpsHist,online}) {
  const s = stats?.consensus||{};
  const sup = stats?.supply||{};
  const threshold = stats?.threshold||{};

  return (
    <div className="pg">
      <div className="c3"><StatCard icon={Zap}    label="Round"       value={fmtN(s.round)}            color="var(--red)"/></div>
      <div className="c3"><StatCard icon={Users}  label="Validators"  value={fmtN(s.validator_count)}  color="var(--cyan)"/></div>
      <div className="c3"><StatCard icon={Database} label="Vertices"  value={fmtN(s.total_vertices)} sub={`${fmtN(s.committed_vertices)} committed`} color="var(--purple)"/></div>
      <div className="c3"><StatCard icon={Activity} label="Total TXs" value={fmtN(s.tx_count)}         color="var(--yellow)"/></div>
      <div className="c3"><StatCard icon={Users}  label="Accounts"    value={fmtN(s.account_count)}    color="var(--green)"/></div>
      <div className="c3"><StatCard icon={Clock}  label="Pending TXs" value={fmtN(s.pending_txs)}      color="var(--txm)"/></div>
      <div className="c3"><StatCard icon={TrendingUp} label="Fee Pool" value={fmtRF(sup.fee_pool||0)}  color="var(--yellow)"/></div>
      <div className="c3"><StatCard icon={Lock}   label="Threshold"   value={`Rnd ${threshold.round||0}`} sub={threshold.algorithm||'ML-KEM-768'} color="var(--cyan)"/></div>

      {/* TPS Chart */}
      <div className="c8">
        <div className="card fi">
          <SectionHdr icon={BarChart3} title="Transaction Activity" color="var(--red)"/>
          <ResponsiveContainer width="100%" height={180}>
            <AreaChart data={tpsHist}>
              <defs>
                <linearGradient id="rg" x1="0" y1="0" x2="0" y2="1">
                  <stop offset="5%" stopColor="var(--red)" stopOpacity={0.25}/>
                  <stop offset="95%" stopColor="var(--red)" stopOpacity={0}/>
                </linearGradient>
              </defs>
              <CartesianGrid strokeDasharray="3 3" stroke="rgba(255,255,255,.05)"/>
              <XAxis dataKey="t" tick={{fill:'var(--txl)',fontSize:10}} tickLine={false}/>
              <YAxis tick={{fill:'var(--txl)',fontSize:10}} tickLine={false} axisLine={false}/>
              <Tooltip contentStyle={{background:'var(--bg2)',border:'1px solid var(--border)',borderRadius:8,fontSize:12}}/>
              <Area type="monotone" dataKey="v" stroke="var(--red)" fill="url(#rg)" strokeWidth={2} dot={false}/>
            </AreaChart>
          </ResponsiveContainer>
        </div>
      </div>

      {/* Supply */}
      <div className="c4">
        <div className="card fi">
          <SectionHdr icon={TrendingUp} title="Token Supply" color="var(--yellow)"/>
          {[
            ['Total',      sup.total,       'var(--txt)'],
            ['Circulating',sup.circulating, 'var(--green)'],
            ['Fee Pool',   sup.fee_pool,    'var(--yellow)'],
            ['Faucet',     sup.faucet,      'var(--cyan)'],
          ].map(([lbl,val,c])=>(
            <div key={lbl} style={{display:'flex',justifyContent:'space-between',padding:'8px 0',borderBottom:'1px solid var(--border)',fontSize:13}}>
              <span style={{color:'var(--txm)'}}>{lbl}</span>
              <span style={{color:c,fontWeight:600}}>{fmtN(val)} RF</span>
            </div>
          ))}
        </div>
      </div>

      {/* DAG Table */}
      <div className="c8">
        <div className="card fi np">
          <div style={{padding:'16px 18px 0'}}>
            <SectionHdr icon={Globe} title="Live DAG" color="var(--cyan)"
              right={<Chip color="c">Bullshark</Chip>}/>
          </div>
          <div style={{overflowX:'auto'}}>
            <table className="tbl">
              <thead><tr><th>Vertex</th><th>Round</th><th>Author</th><th>TXs</th><th>Status</th></tr></thead>
              <tbody>
                {vertices.slice(0,10).map(v=>(
                  <tr key={v.id}>
                    <td className="mono" style={{color:'var(--cyan)'}} title={v.id}>0x{v.id.slice(0,14)}…</td>
                    <td className="bold">{v.round}</td>
                    <td className="mono">{v.author.slice(0,12)}…</td>
                    <td>{v.tx_count}</td>
                    <td>{v.committed ? <Chip color="g">✓ Committed</Chip> : <Chip color="x">Pending</Chip>}</td>
                  </tr>
                ))}
                {vertices.length===0 && <tr><td colSpan={5} style={{textAlign:'center',color:'var(--txl)',padding:24}}>Waiting for vertices…</td></tr>}
              </tbody>
            </table>
          </div>
        </div>
      </div>

      {/* Mempool */}
      <div className="c4">
        <div className="card fi">
          <SectionHdr icon={Activity} title="Mempool" color="var(--red)"
            right={<Chip color="r">{mempool.count||0} pending</Chip>}/>
          <div style={{maxHeight:240,overflowY:'auto'}}>
            {(mempool.txs||[]).slice(0,8).map((tx,i)=>(
              <div key={i} style={{padding:'8px 0',borderBottom:'1px solid rgba(255,255,255,.04)',fontSize:12}}>
                <div style={{display:'flex',justifyContent:'space-between'}}>
                  <span className="font-mono" style={{fontSize:10,color:'var(--txm)'}}>{short(tx.sender,10)}→{short(tx.receiver,10)}</span>
                  <span style={{color:'var(--red)',fontWeight:600}}>{fmtN(tx.amount)} RF</span>
                </div>
                <div style={{color:'var(--txl)',fontSize:10,marginTop:2}}>fee:{tx.fee} · nonce:{tx.nonce}</div>
              </div>
            ))}
            {(!mempool.txs||mempool.txs.length===0)&&<div style={{textAlign:'center',color:'var(--txl)',padding:'20px 0',fontSize:12}}>Mempool empty</div>}
          </div>
        </div>
      </div>

      {/* Threshold */}
      <div className="c12">
        <div className="card cc fi">
          <div style={{display:'flex',alignItems:'center',gap:16,flexWrap:'wrap'}}>
            <Lock size={20} color="var(--cyan)"/>
            <div>
              <div style={{fontWeight:700,marginBottom:2}}>Threshold Encrypted Mempool — ML-KEM-768</div>
              <div style={{fontSize:11,color:'var(--txm)'}}>Anti-MEV • Anti-frontrunning • Keys rotated every round</div>
            </div>
            <Chip color="c">Round {roundEk.round||0}</Chip>
            <div style={{fontFamily:'var(--mono)',fontSize:10,color:'var(--txl)',wordBreak:'break-all',flex:1}}>
              EK: {(roundEk.ek_hex||'').slice(0,64)}…
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

// ─────────────────────────────────────────────
// EXPLORER PAGE
// ─────────────────────────────────────────────
function ExplorerPage({initialQuery='', wsData}) {
  const [q,setQ] = useState(initialQuery);
  const [result,setResult] = useState(null);
  const [loading,setLoading] = useState(false);
  const [vertices,setVertices] = useState([]);
  const [recentTxs,setRecentTxs] = useState([]);
  const [page,setPage] = useState(0);
  const PAGE_SIZE = 20;
  const [tab,setTab] = useState('blocks'); // 'blocks' | 'txs' | 'search'
  const [selectedVertex,setSelectedVertex] = useState(null); // eslint-disable-line no-unused-vars

  const loadVertices = useCallback(async()=>{
    const v = await sdk.getVertices().catch(()=>[]);
    setVertices(v||[]);
    // flatten txs from all vertices
    const txs = (v||[]).flatMap(vx=>(vx.transactions||[]).map(tx=>({...tx,vertex_id:vx.id,round:vx.round,committed:vx.committed})));
    txs.sort((a,b)=>(b.timestamp||b.round||0)-(a.timestamp||a.round||0));
    setRecentTxs(txs);
  }, []);

  // Fallback poll 30s; WS new_block fires instantly
  useEffect(()=>{ loadVertices(); const id=setInterval(loadVertices,30000); return()=>clearInterval(id); },[loadVertices]);
  useEffect(()=>{ if(wsData?.type==='new_block') loadVertices(); },[wsData, loadVertices]);

  useEffect(()=>{
    if(!initialQuery) return;
    setTab('search');
    setLoading(true); setResult(null);
    sdk.search(initialQuery.trim())
      .then(r=>setResult(r))
      .catch(ex=>setResult({type:'error',msg:ex.message}))
      .finally(()=>setLoading(false));
  },[initialQuery]);

  const search = async(e)=>{
    e.preventDefault(); if(!q.trim()) return;
    setTab('search');
    setLoading(true); setResult(null);
    try { const r = await sdk.search(q.trim()); setResult(r); }
    catch(ex){ setResult({type:'error',msg:ex.message}); }
    finally { setLoading(false); }
  };

  const openVertex = (v)=>{
    setSelectedVertex(v);
    setTab('search');
    setQ(v.id);
    setResult({
      type:'vertex',
      id: v.id,
      round: v.round,
      author: v.author,
      tx_count: v.tx_count,
      etx_count: v.etx_count||0,
      committed: v.committed,
      transactions: v.transactions||[],
    });
  };

  const tabStyle = (t)=>({
    padding:'8px 18px', cursor:'pointer', fontSize:13, fontWeight:600,
    borderBottom: tab===t ? '2px solid var(--cyan)' : '2px solid transparent',
    color: tab===t ? 'var(--cyan)' : 'var(--txm)',
    background:'none', border:'none', borderBottomWidth:2,
    borderBottomStyle:'solid',
  });

  return (
    <div>
      {/* Search bar */}
      <form onSubmit={search} style={{display:'flex',gap:12,marginBottom:16}}>
        <input className="inp" placeholder="Search address, vertex ID or tx hash…" value={q} onChange={e=>setQ(e.target.value)} style={{flex:1}}/>
        <button type="submit" className="btn btn-p" disabled={loading||!q.trim()}>
          {loading ? <span className="spin"><RefreshCw size={14}/></span> : <Search size={14}/>} Search
        </button>
      </form>

      {/* Tabs */}
      <div style={{display:'flex',borderBottom:'1px solid var(--border)',marginBottom:20}}>
        <button style={tabStyle('blocks')} onClick={()=>{ setTab('blocks'); setPage(0); }}>Live Blocks</button>
        <button style={tabStyle('txs')} onClick={()=>{ setTab('txs'); setPage(0); }}>
          Recent TXs {recentTxs.length>0 && <span style={{marginLeft:4,background:'var(--cyan)',color:'#000',borderRadius:10,padding:'0 5px',fontSize:10,fontWeight:700}}>{recentTxs.length}</span>}
        </button>
        <button style={tabStyle('search')} onClick={()=>setTab('search')}>Search</button>
      </div>

      {/* Search results tab */}
      {tab==='search' && result && (
        <div className="card fi" style={{marginBottom:24}}>
          {result.type==='address' && (
            <div>
              <div style={{display:'flex',alignItems:'center',gap:10,marginBottom:16}}>
                <Wallet size={18} color="var(--cyan)"/>
                <span style={{fontFamily:'var(--acc)',fontWeight:700,fontSize:16}}>Account</span>
                <Chip color="c">address</Chip>
              </div>
              {[['Address',result.address],['Balance',fmtRF(result.balance)],['Nonce',result.nonce],['Transactions',result.tx_count],['Total Sent',fmtRF(result.total_sent)],['Total Received',fmtRF(result.total_received)]].map(([k,v])=>(
                <div key={k} style={{display:'flex',justifyContent:'space-between',padding:'8px 0',borderBottom:'1px solid var(--border)',fontSize:13}}>
                  <span style={{color:'var(--txm)'}}>{k}</span>
                  <span style={{fontFamily:k==='Address'?'var(--mono)':'inherit',fontSize:k==='Address'?11:13,color:'var(--txt)',fontWeight:600,wordBreak:'break-all',maxWidth:'65%',textAlign:'right'}}>{v}</span>
                </div>
              ))}
              {result.history?.length>0 && (
                <div style={{marginTop:16}}>
                  <div style={{fontWeight:700,marginBottom:10,fontSize:13,color:'var(--txm)'}}>Recent Transactions</div>
                  <table className="tbl">
                    <thead><tr><th>Sender</th><th>Receiver</th><th>Amount</th><th>Fee</th><th>Date</th></tr></thead>
                    <tbody>
                      {result.history.map((tx,i)=>(
                        <tr key={i}>
                          <td className="mono" style={{cursor:'pointer',color:'var(--cyan)'}} onClick={()=>{setQ(tx.sender);search({preventDefault:()=>{}})}}>{short(tx.sender,14)}</td>
                          <td className="mono" style={{cursor:'pointer',color:'var(--cyan)'}} onClick={()=>{setQ(tx.receiver);search({preventDefault:()=>{}})}}>{short(tx.receiver,14)}</td>
                          <td className="bold">{fmtN(tx.amount)} RF</td>
                          <td style={{color:'var(--txl)'}}>{tx.fee}</td>
                          <td style={{color:'var(--txl)',fontSize:11}}>{fmtDate(tx.timestamp)}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </div>
          )}
          {result.type==='vertex' && (
            <div>
              <div style={{display:'flex',alignItems:'center',gap:10,marginBottom:16}}>
                <Database size={18} color="var(--purple)"/>
                <span style={{fontFamily:'var(--acc)',fontWeight:700,fontSize:16}}>DAG Block</span>
                {result.committed ? <Chip color="g">Committed</Chip> : <Chip color="x">Pending</Chip>}
              </div>
              {[['ID',result.id],['Round',result.round],['Author',result.author],['Transactions',result.tx_count],['Encrypted TXs',result.etx_count]].map(([k,v])=>(
                <div key={k} style={{display:'flex',justifyContent:'space-between',padding:'8px 0',borderBottom:'1px solid var(--border)',fontSize:13}}>
                  <span style={{color:'var(--txm)'}}>{k}</span>
                  <span style={{fontFamily:['ID','Author'].includes(k)?'var(--mono)':'inherit',fontSize:['ID','Author'].includes(k)?11:13,color:'var(--txt)',fontWeight:600,wordBreak:'break-all',maxWidth:'65%',textAlign:'right'}}>{String(v)}</span>
                </div>
              ))}
              {result.transactions?.length>0 && (
                <div style={{marginTop:16}}>
                  <div style={{fontWeight:700,marginBottom:10,fontSize:13,color:'var(--txm)'}}>Transactions in this block</div>
                  <table className="tbl">
                    <thead><tr><th>Sender</th><th>Receiver</th><th>Amount</th><th>Fee</th></tr></thead>
                    <tbody>
                      {result.transactions.map((tx,i)=>(
                        <tr key={i}>
                          <td className="mono">{short(tx.sender||'',14)}</td>
                          <td className="mono">{short(tx.receiver||'',14)}</td>
                          <td className="bold">{fmtN(tx.amount||0)} RF</td>
                          <td style={{color:'var(--txl)'}}>{tx.fee||0}</td>
                        </tr>
                      ))}
                    </tbody>
                  </table>
                </div>
              )}
            </div>
          )}
          {result.type==='not_found' && <Alert type="wrn">No results for: <b>{result.query||result.hash}</b></Alert>}
          {result.type==='error'     && <Alert type="err">{result.msg}</Alert>}
        </div>
      )}
      {tab==='search' && !result && !loading && (
        <div style={{textAlign:'center',color:'var(--txl)',padding:40}}>Enter an address, vertex ID or tx hash to search</div>
      )}

      {/* Recent TXs tab */}
      {tab==='txs' && (
        <div className="card np">
          <div style={{padding:'16px 18px 10px',display:'flex',alignItems:'center',justifyContent:'space-between'}}>
            <SectionHdr icon={ArrowUpRight} title="Recent Transactions" color="var(--green)"/>
            <span style={{fontSize:11,color:'var(--txl)'}}>{recentTxs.length} transactions</span>
          </div>
          <div style={{overflowX:'auto'}}>
            <table className="tbl">
              <thead><tr><th>Round</th><th>From</th><th>To</th><th>Amount</th><th>Fee</th><th>Time</th></tr></thead>
              <tbody>
                {recentTxs.slice(page*PAGE_SIZE,(page+1)*PAGE_SIZE).map((tx,i)=>(
                  <tr key={i}>
                    <td className="bold" style={{color:'var(--cyan)'}}>{tx.round||'—'}</td>
                    <td className="mono" style={{cursor:'pointer',color:'var(--cyan)'}} onClick={()=>{setQ(tx.sender);setTab('search');sdk.search(tx.sender).then(setResult);}}>{short(tx.sender||'',12)}</td>
                    <td className="mono" style={{cursor:'pointer',color:'var(--purple)'}} onClick={()=>{setQ(tx.receiver);setTab('search');sdk.search(tx.receiver).then(setResult);}}>{short(tx.receiver||'',12)}</td>
                    <td className="bold">{fmtN(tx.amount||0)} RF</td>
                    <td style={{color:'var(--txl)'}}>{tx.fee||0}</td>
                    <td style={{color:'var(--txl)',fontSize:11}}>{tx.timestamp ? fmtDate(tx.timestamp) : '—'}</td>
                  </tr>
                ))}
                {recentTxs.length===0 && <tr><td colSpan={6} style={{textAlign:'center',color:'var(--txl)',padding:24}}>No transactions yet</td></tr>}
              </tbody>
            </table>
          </div>
          {recentTxs.length > PAGE_SIZE && (
            <div style={{display:'flex',justifyContent:'center',gap:8,padding:'12px 0'}}>
              <button className="btn" disabled={page===0} onClick={()=>setPage(p=>p-1)} style={{fontSize:12}}>← Prev</button>
              <span style={{lineHeight:'32px',fontSize:12,color:'var(--txm)'}}>Page {page+1} / {Math.ceil(recentTxs.length/PAGE_SIZE)}</span>
              <button className="btn" disabled={(page+1)*PAGE_SIZE>=recentTxs.length} onClick={()=>setPage(p=>p+1)} style={{fontSize:12}}>Next →</button>
            </div>
          )}
        </div>
      )}

      {/* Live blocks tab */}
      {tab==='blocks' && (
        <div className="card np">
          <div style={{padding:'16px 18px 10px',display:'flex',alignItems:'center',justifyContent:'space-between'}}>
            <SectionHdr icon={Database} title="Live Block Feed" color="var(--cyan)"/>
            <span style={{fontSize:11,color:'var(--txl)'}}>{vertices.length} blocks</span>
          </div>
          <div style={{overflowX:'auto'}}>
            <table className="tbl">
              <thead><tr><th>Round</th><th>Vertex ID</th><th>Author</th><th>TXs</th><th>Status</th></tr></thead>
              <tbody>
                {vertices.slice().sort((a,b)=>b.round-a.round).slice(page*PAGE_SIZE,(page+1)*PAGE_SIZE).map(v=>(
                  <tr key={v.id} style={{cursor:'pointer'}} onClick={()=>openVertex(v)}>
                    <td className="bold" style={{color:'var(--cyan)'}}>{v.round}</td>
                    <td className="mono" title={v.id}>0x{v.id.slice(0,16)}…</td>
                    <td className="mono">{v.author ? v.author.slice(0,14)+'…' : '—'}</td>
                    <td>
                      <span style={{background:'var(--purple)',color:'#fff',borderRadius:4,padding:'2px 7px',fontSize:11,fontWeight:700}}>
                        {v.tx_count}
                      </span>
                    </td>
                    <td>{v.committed ? <Chip color="g">✓ Committed</Chip> : <Chip color="x">⏳ Pending</Chip>}</td>
                  </tr>
                ))}
                {vertices.length===0 && <tr><td colSpan={5} style={{textAlign:'center',color:'var(--txl)',padding:32}}>Waiting for blocks…</td></tr>}
              </tbody>
            </table>
          </div>
          {vertices.length > PAGE_SIZE && (
            <div style={{display:'flex',justifyContent:'center',gap:8,padding:'12px 0'}}>
              <button className="btn" disabled={page===0} onClick={()=>setPage(p=>p-1)} style={{fontSize:12}}>← Prev</button>
              <span style={{lineHeight:'32px',fontSize:12,color:'var(--txm)'}}>Page {page+1} / {Math.ceil(vertices.length/PAGE_SIZE)}</span>
              <button className="btn" disabled={(page+1)*PAGE_SIZE>=vertices.length} onClick={()=>setPage(p=>p+1)} style={{fontSize:12}}>Next →</button>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

// ─────────────────────────────────────────────
// NETWORK PAGE
// ─────────────────────────────────────────────
function NetworkPage({stats,online}) {
  const n = stats?.node||{};
  const c = stats?.consensus||{};
  const t = stats?.threshold||{};
  const s = stats?.supply||{};
  const uptime = n.uptime_secs||0;
  const uptimeStr = `${Math.floor(uptime/3600)}h ${Math.floor((uptime%3600)/60)}m`;

  return (
    <div className="pg">
      <div className="c6">
        <div className="card fi">
          <SectionHdr icon={Globe} title="Node Identity"/>
          {[['Peer ID',n.peer_id||'—'],['Chain ID',n.chain_id||2100],['Version',n.version||'2.1.0'],['Uptime',uptimeStr],['Min Fee',`${n.min_fee||1} RF`]].map(([k,v])=>(
            <div key={k} style={{display:'flex',justifyContent:'space-between',alignItems:'center',padding:'10px 0',borderBottom:'1px solid var(--border)',fontSize:13}}>
              <span style={{color:'var(--txm)'}}>{k}</span>
              <div style={{display:'flex',alignItems:'center',gap:6}}>
                <span style={{fontFamily:k==='Peer ID'?'var(--mono)':'inherit',fontSize:k==='Peer ID'?10:13,color:'var(--txt)',wordBreak:'break-all',textAlign:'right',maxWidth:260}}>{String(v)}</span>
                {k==='Peer ID' && <CopyBtn text={n.peer_id||''}/>}
              </div>
            </div>
          ))}
        </div>
      </div>

      <div className="c6">
        <div className="card fi">
          <SectionHdr icon={Cpu} title="Consensus Stats" color="var(--purple)"/>
          {[['Round',c.round],['Validators',c.validator_count],['Total Vertices',c.total_vertices],['Committed',c.committed_vertices],['Pending TXs',c.pending_txs],['Total TXs',c.tx_count]].map(([k,v])=>(
            <div key={k} style={{display:'flex',justifyContent:'space-between',padding:'10px 0',borderBottom:'1px solid var(--border)',fontSize:13}}>
              <span style={{color:'var(--txm)'}}>{k}</span>
              <span style={{color:'var(--txt)',fontWeight:600}}>{fmtN(v)}</span>
            </div>
          ))}
        </div>
      </div>

      <div className="c6">
        <div className="card cc fi">
          <SectionHdr icon={Shield} title="Threshold Encryption" color="var(--cyan)"/>
          <div style={{marginBottom:16}}>
            <Chip color="c">ML-KEM-768 (FIPS 203)</Chip>
            <div style={{marginTop:12,fontSize:13}}>
              <div style={{display:'flex',justifyContent:'space-between',padding:'8px 0',borderBottom:'1px solid rgba(255,255,255,.05)'}}>
                <span style={{color:'var(--txm)'}}>Active Round</span>
                <span style={{fontWeight:700}}>{t.round||0}</span>
              </div>
              <div style={{padding:'8px 0',borderBottom:'1px solid rgba(255,255,255,.05)'}}>
                <div style={{color:'var(--txm)',marginBottom:6,fontSize:12}}>Current EK prefix</div>
                <div style={{fontFamily:'var(--mono)',fontSize:10,color:'var(--cyan)',wordBreak:'break-all'}}>{t.ek_prefix||'—'}…</div>
              </div>
            </div>
          </div>
          <div className="alt alt-inf" style={{fontSize:12}}>
            <Lock size={13}/>All transactions encrypted before entering mempool. Decrypted only after Bullshark commit.
          </div>
        </div>
      </div>

      <div className="c6">
        <div className="card fi">
          <SectionHdr icon={TrendingUp} title="Token Economics" color="var(--yellow)"/>
          {[['Total Supply',fmtRF(s.total)],['Circulating',fmtRF(s.circulating)],['Fee Pool',fmtRF(s.fee_pool)],['Faucet Reserve',fmtRF(s.faucet)],['Genesis',fmtRF(s.genesis)]].map(([k,v])=>(
            <div key={k} style={{display:'flex',justifyContent:'space-between',padding:'9px 0',borderBottom:'1px solid var(--border)',fontSize:13}}>
              <span style={{color:'var(--txm)'}}>{k}</span>
              <span style={{color:'var(--txt)',fontWeight:600}}>{v}</span>
            </div>
          ))}
        </div>
      </div>

      <div className="c12">
        <div className="card fi">
          <SectionHdr icon={Cpu} title="Tech Stack"/>
          <div style={{display:'grid',gridTemplateColumns:'repeat(4,1fr)',gap:12}}>
            {[
              {lbl:'Consensus',val:'Bullshark DAG',sub:'Narwhal+Bullshark',c:'var(--red)'},
              {lbl:'Signatures',val:'ML-DSA-65',sub:'FIPS 204 Post-Quantum',c:'var(--cyan)'},
              {lbl:'KEM',val:'ML-KEM-768',sub:'FIPS 203 Anti-MEV',c:'var(--purple)'},
              {lbl:'P2P',val:'libp2p 0.53',sub:'QUIC+TCP+Kademlia+mDNS',c:'var(--yellow)'},
              {lbl:'Execution',val:'Parallel',sub:'rayon + conflict detection',c:'var(--green)'},
              {lbl:'VM',val:'WASM (wasmi)',sub:'Gas metering + ML-DSA verify',c:'var(--purple)'},
              {lbl:'Storage',val:'sled',sub:'Embedded persistent DB',c:'var(--txm)'},
              {lbl:'Crypto',val:'aws-lc-rs',sub:'FIPS 140-3 validated',c:'var(--red)'},
            ].map(t=>(
              <div key={t.lbl} style={{padding:'14px',background:'var(--bg3)',borderRadius:'var(--rs)',border:'1px solid var(--border)'}}>
                <div style={{fontSize:10,color:'var(--txl)',fontWeight:700,letterSpacing:.8,marginBottom:6}}>{t.lbl}</div>
                <div style={{fontWeight:700,color:t.c,fontSize:14}}>{t.val}</div>
                <div style={{fontSize:11,color:'var(--txm)',marginTop:3}}>{t.sub}</div>
              </div>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}

// ─────────────────────────────────────────────
// SETTINGS PAGE
// ─────────────────────────────────────────────
function SettingsPage({wallet,onLogout}) {
  const [exported,setExported] = useState(false);
  const exportKeystore = () => {
    const ks = loadKeystore();
    const blob = new Blob([JSON.stringify(ks,null,2)], {type:'application/json'});
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a'); a.href=url; a.download='redflag-keystore.json'; a.click();
    URL.revokeObjectURL(url); setExported(true);
  };
  return (
    <div className="pg">
      <div className="c6">
        <div className="card fi">
          <SectionHdr icon={Settings} title="Wallet Security"/>
          <div className="field">
            <div className="inp-lbl">Your Address</div>
            <div style={{display:'flex',alignItems:'center',gap:8}}>
              <div className="code" style={{flex:1,fontSize:10}}>{wallet?.address||'—'}</div>
              <CopyBtn text={wallet?.address||''}/>
            </div>
          </div>
          <div className="field">
            <div className="inp-lbl">Seed Phrase</div>
            <details>
              <summary style={{cursor:'pointer',fontSize:12,color:'var(--txl)'}}>Show recovery phrase</summary>
              <div className="mn-grid" style={{marginTop:12}}>
                {(wallet?.mnemonic||'').split(' ').map((w,i)=>(
                  <div key={i} className="mn-word">
                    <span className="mn-i">{i+1}.</span>
                    <span className="mn-w">{w}</span>
                  </div>
                ))}
              </div>
            </details>
          </div>
          <div className="dvd"/>
          <div style={{display:'flex',gap:10,flexWrap:'wrap'}}>
            <button className="btn btn-ghost" onClick={exportKeystore}><Download size={14}/>Export Keystore</button>
            {exported && <Chip color="g">✓ Downloaded</Chip>}
          </div>
        </div>
      </div>
      <div className="c6">
        <div className="card cr fi">
          <SectionHdr icon={AlertTriangle} title="Danger Zone" color="var(--red)"/>
          <div className="alt alt-wrn" style={{marginBottom:16}}>
            <AlertTriangle size={14}/>Removing wallet will delete all local data. Make sure you have your seed phrase.
          </div>
          <button className="btn btn-o" style={{color:'var(--red)',borderColor:'var(--br)'}} onClick={()=>{if(confirm('Delete wallet?')){deleteKeystore();onLogout();}}}>
            <LogOut size={14}/> Remove Wallet
          </button>
        </div>
      </div>
    </div>
  );
}

// ─────────────────────────────────────────────
// ROOT APP
// ─────────────────────────────────────────────
// DEX PAGE
// ─────────────────────────────────────────────
function DexPage({ wallet, wsData }) {
  const [pools, setPools]       = useState([]);
  const [selPool, setSelPool]   = useState('RF_wETH');
  const [tab, setTab]           = useState('swap');   // swap | liquidity | pools
  const [dir, setDir]           = useState('rf_to_b');
  const [amtIn, setAmtIn]       = useState('');
  const [quote, setQuote]       = useState(null);
  const [history, setHistory]   = useState([]);
  const [prices, setPrices]     = useState([]);
  const [busy, setBusy]         = useState(false);
  const [msg, setMsg]           = useState(null);
  const [amtRF, setAmtRF]       = useState('');
  const [amtB, setAmtB]         = useState('');
  const [lpAmt, setLpAmt]       = useState('');
  const [position, setPosition] = useState(null);

  const pool = pools.find(p => p.pool_id === selPool) || {};
  const tokenB = selPool.replace('RF_','');

  const fetchData = useCallback(async () => {
    try {
      const [pd, hd, prd] = await Promise.all([
        sdk.getDexPools(),
        sdk.getDexHistory(selPool),
        sdk.getDexPrices(selPool),
      ]);
      setPools(pd.pools || []);
      setHistory(hd.swaps || []);
      setPrices(prd.prices || []);
      if (wallet) {
        const pos = await sdk.getDexPosition(wallet.address, selPool);
        setPosition(pos);
      }
    } catch {}
  }, [selPool, wallet]);

  useEffect(() => { fetchData(); const iv = setInterval(fetchData, 30000); return () => clearInterval(iv); }, [fetchData]);
  useEffect(() => { if(wsData?.type==='dex_swap'||wsData?.type==='dex_liquidity'||wsData?.type==='new_block') fetchData(); }, [wsData, fetchData]);

  // Auto-quote
  useEffect(() => {
    if (!amtIn || isNaN(amtIn) || Number(amtIn) <= 0) { setQuote(null); return; }
    const t = setTimeout(async () => {
      try { const q = await sdk.dexQuote(selPool, dir, Number(amtIn)); setQuote(q); } catch { setQuote(null); }
    }, 400);
    return () => clearTimeout(t);
  }, [amtIn, dir, selPool]);

  const notify = (type, text) => { setMsg({ type, text }); setTimeout(() => setMsg(null), 4000); };

  const handleSwap = async () => {
    if (!wallet) return notify('err','Conecta tu wallet primero');
    if (!amtIn || Number(amtIn) <= 0) return notify('err','Ingresa un monto');
    setBusy(true);
    try {
      const minOut = quote ? Math.floor(quote.amount_out * 0.99) : 0; // 1% slippage
      const res = await sdk.dexSwap(wallet.private_key_hex, selPool, dir, Number(amtIn), minOut);
      if (res.success) {
        notify('ok', `✅ Swap: ${amtIn} ${dir==='rf_to_b'?'RF':''+tokenB} → ${res.amount_out} ${dir==='rf_to_b'?tokenB:'RF'}`);
        setAmtIn(''); setQuote(null); fetchData();
      } else notify('err', res.error || 'Error en el swap');
    } catch(e) { notify('err', e.response?.data?.error || e.message); }
    setBusy(false);
  };

  const handleAddLiq = async () => {
    if (!wallet) return notify('err','Conecta tu wallet primero');
    setBusy(true);
    try {
      const res = await sdk.dexAddLiquidity(wallet.private_key_hex, selPool, Number(amtRF), Number(amtB));
      if (res.success) { notify('ok', `✅ Liquidez añadida: ${res.lp_tokens} LP tokens`); setAmtRF(''); setAmtB(''); fetchData(); }
      else notify('err', res.error || 'Error');
    } catch(e) { notify('err', e.response?.data?.error || e.message); }
    setBusy(false);
  };

  const handleRemoveLiq = async () => {
    if (!wallet) return notify('err','Conecta tu wallet primero');
    setBusy(true);
    try {
      const res = await sdk.dexRemoveLiquidity(wallet.private_key_hex, selPool, Number(lpAmt));
      if (res.success) { notify('ok', `✅ Retirado: ${res.amount_rf} RF + ${res.amount_b} ${tokenB}`); setLpAmt(''); fetchData(); }
      else notify('err', res.error || 'Error');
    } catch(e) { notify('err', e.response?.data?.error || e.message); }
    setBusy(false);
  };

  const priceChartData = prices.slice(-60).map(p => ({ t: new Date(p.ts*1000).toLocaleTimeString('en',{hour:'2-digit',minute:'2-digit'}), price: p.price }));

  return (
    <div style={{display:'flex',flexDirection:'column',gap:16}}>
      <SectionHdr icon={ArrowLeftRight} title="DEX — redflag.web3 Swap" color="var(--purple)"
        right={<div style={{display:'flex',gap:8}}>
          {['swap','liquidity','pools'].map(t=>(
            <button key={t} className={`btn-o ${tab===t?'active':''}`} style={tab===t?{borderColor:'var(--purple)',color:'var(--purple)'}:{}} onClick={()=>setTab(t)}>{t}</button>
          ))}
        </div>}
      />

      {msg && <Alert type={msg.type}>{msg.text}</Alert>}

      <div style={{display:'grid',gridTemplateColumns:'1fr 360px',gap:16}}>

        {/* CHART + STATS */}
        <div style={{display:'flex',flexDirection:'column',gap:12}}>
          {/* Pool selector */}
          <div className="card fi" style={{display:'flex',gap:8,alignItems:'center'}}>
            {pools.map(p=>(
              <button key={p.pool_id} onClick={()=>setSelPool(p.pool_id)}
                className={`btn-o ${selPool===p.pool_id?'active':''}`}
                style={selPool===p.pool_id?{borderColor:'var(--purple)',color:'var(--purple)',fontWeight:700}:{}}>
                RF / {p.token_b}
              </button>
            ))}
          </div>

          {/* Price chart */}
          <div className="card fi" style={{height:220}}>
            <div style={{display:'flex',justifyContent:'space-between',marginBottom:8}}>
              <span style={{fontWeight:700,color:'var(--purple)'}}>RF / {tokenB} Price</span>
              <span style={{fontSize:13,color:'var(--green)',fontWeight:700}}>{pool.price ? pool.price.toFixed(6) : '—'} {tokenB}</span>
            </div>
            <ResponsiveContainer width="100%" height={160}>
              <AreaChart data={priceChartData}>
                <defs><linearGradient id="pg" x1="0" y1="0" x2="0" y2="1"><stop offset="5%" stopColor="var(--purple)" stopOpacity={0.3}/><stop offset="95%" stopColor="var(--purple)" stopOpacity={0}/></linearGradient></defs>
                <XAxis dataKey="t" tick={{fontSize:10,fill:'var(--txl)'}} tickLine={false}/>
                <YAxis tick={{fontSize:10,fill:'var(--txl)'}} tickLine={false} axisLine={false} domain={['auto','auto']}/>
                <Tooltip contentStyle={{background:'var(--bg2)',border:'1px solid var(--bdr)',borderRadius:8,fontSize:11}} formatter={v=>[v?.toFixed(6),'Price']}/>
                <CartesianGrid stroke="var(--bdr)" strokeDasharray="3 3" vertical={false}/>
                <Area type="monotone" dataKey="price" stroke="var(--purple)" fill="url(#pg)" strokeWidth={2} dot={false}/>
              </AreaChart>
            </ResponsiveContainer>
          </div>

          {/* Pool stats */}
          <div style={{display:'grid',gridTemplateColumns:'repeat(4,1fr)',gap:8}}>
            {[
              {label:'Reserve RF', value:fmtRF(pool.reserve_rf)},
              {label:`Reserve ${tokenB}`, value:fmtN(pool.reserve_b)},
              {label:'Volume RF', value:fmtRF(pool.volume_rf)},
              {label:'Fees collected', value:fmtRF(pool.fees_collected)},
            ].map(s=>(
              <div key={s.label} className="card fi" style={{padding:'10px 14px'}}>
                <div style={{fontSize:10,color:'var(--txl)',marginBottom:4}}>{s.label}</div>
                <div style={{fontWeight:700,fontSize:13,color:'var(--purple)'}}>{s.value}</div>
              </div>
            ))}
          </div>

          {/* Swap history */}
          <div className="card fi">
            <div style={{fontWeight:700,marginBottom:10,fontSize:13}}>Recent Trades</div>
            <table className="tbl"><thead><tr><th>Type</th><th>Amount In</th><th>Amount Out</th><th>Trader</th><th>Time</th></tr></thead>
              <tbody>
                {history.slice(0,20).map((s,i)=>(
                  <tr key={i}>
                    <td><span className={`bdg ${s.direction==='RfToB'?'bdg-r':'bdg-g'}`}>{s.direction==='RfToB'?`RF→${tokenB}`:`${tokenB}→RF`}</span></td>
                    <td>{fmtN(s.amount_in)}</td>
                    <td style={{color:'var(--green)'}}>{fmtN(s.amount_out)}</td>
                    <td style={{fontFamily:'monospace',fontSize:11}}>{short(s.trader,8)}</td>
                    <td style={{color:'var(--txl)',fontSize:11}}>{fmtDate(s.timestamp)}</td>
                  </tr>
                ))}
                {history.length===0 && <tr><td colSpan={5} style={{textAlign:'center',color:'var(--txl)',padding:20}}>No trades yet</td></tr>}
              </tbody>
            </table>
          </div>
        </div>

        {/* SIDE PANEL */}
        <div style={{display:'flex',flexDirection:'column',gap:12}}>

          {/* ── SWAP ── */}
          {tab === 'swap' && (
            <div className="card fi" style={{display:'flex',flexDirection:'column',gap:12}}>
              <div style={{fontWeight:700,fontSize:14,color:'var(--purple)',display:'flex',alignItems:'center',gap:8}}>
                <ArrowLeftRight size={16}/> Swap
              </div>
              {/* Direction toggle */}
              <div style={{display:'flex',gap:6}}>
                <button className={`btn-o ${dir==='rf_to_b'?'active':''}`}
                  style={dir==='rf_to_b'?{borderColor:'var(--purple)',color:'var(--purple)',flex:1}:{flex:1}}
                  onClick={()=>{setDir('rf_to_b');setAmtIn('');setQuote(null);}}>
                  RF → {tokenB}
                </button>
                <button className={`btn-o ${dir==='b_to_rf'?'active':''}`}
                  style={dir==='b_to_rf'?{borderColor:'var(--green)',color:'var(--green)',flex:1}:{flex:1}}
                  onClick={()=>{setDir('b_to_rf');setAmtIn('');setQuote(null);}}>
                  {tokenB} → RF
                </button>
              </div>

              <div>
                <div style={{fontSize:11,color:'var(--txl)',marginBottom:4}}>You pay ({dir==='rf_to_b'?'RF':tokenB})</div>
                <input className="inp" type="number" placeholder="0" value={amtIn} onChange={e=>setAmtIn(e.target.value)} style={{width:'100%',boxSizing:'border-box'}}/>
              </div>

              {quote && (
                <div style={{background:'var(--bg3)',borderRadius:8,padding:'10px 12px',fontSize:12}}>
                  <div style={{display:'flex',justifyContent:'space-between',marginBottom:4}}>
                    <span style={{color:'var(--txl)'}}>You receive</span>
                    <span style={{fontWeight:700,color:'var(--green)'}}>{fmtN(quote.amount_out)} {dir==='rf_to_b'?tokenB:'RF'}</span>
                  </div>
                  <div style={{display:'flex',justifyContent:'space-between',marginBottom:4}}>
                    <span style={{color:'var(--txl)'}}>Fee (0.3%)</span>
                    <span>{fmtN(quote.fee)} {dir==='rf_to_b'?'RF':tokenB}</span>
                  </div>
                  <div style={{display:'flex',justifyContent:'space-between'}}>
                    <span style={{color:'var(--txl)'}}>Price impact</span>
                    <span style={{color:quote.price_impact>2?'var(--red)':'var(--green)'}}>{quote.price_impact?.toFixed(2)}%</span>
                  </div>
                </div>
              )}

              {!wallet && <Alert type="wrn">Conecta tu wallet para hacer swap</Alert>}
              <button className="btn-p" style={{background:'var(--purple)',width:'100%'}} onClick={handleSwap} disabled={busy||!wallet}>
                {busy ? <RefreshCw size={14} className="spin"/> : <ArrowLeftRight size={14}/>}
                {busy ? 'Procesando…' : 'Swap'}
              </button>
            </div>
          )}

          {/* ── LIQUIDITY ── */}
          {tab === 'liquidity' && (
            <div className="card fi" style={{display:'flex',flexDirection:'column',gap:12}}>
              <div style={{fontWeight:700,fontSize:14,color:'var(--cyan)',display:'flex',alignItems:'center',gap:8}}>
                <Droplets size={16}/> Liquidity
              </div>
              {position && position.lp_tokens > 0 && (
                <div style={{background:'var(--bg3)',borderRadius:8,padding:'10px 12px',fontSize:12}}>
                  <div style={{color:'var(--txl)',marginBottom:4}}>Tu posición en {selPool}</div>
                  <div style={{fontWeight:700,color:'var(--cyan)'}}>{fmtN(position.lp_tokens)} LP tokens</div>
                </div>
              )}

              <div style={{fontWeight:600,fontSize:12,color:'var(--txl)'}}>Add Liquidity</div>
              <div>
                <div style={{fontSize:11,color:'var(--txl)',marginBottom:4}}>RF amount</div>
                <input className="inp" type="number" placeholder="0" value={amtRF} onChange={e=>setAmtRF(e.target.value)} style={{width:'100%',boxSizing:'border-box'}}/>
              </div>
              <div>
                <div style={{fontSize:11,color:'var(--txl)',marginBottom:4}}>{tokenB} amount</div>
                <input className="inp" type="number" placeholder="0" value={amtB} onChange={e=>setAmtB(e.target.value)} style={{width:'100%',boxSizing:'border-box'}}/>
              </div>
              <button className="btn-g" style={{width:'100%'}} onClick={handleAddLiq} disabled={busy||!wallet}>
                {busy?<RefreshCw size={14} className="spin"/>:<Plus size={14}/>} Add Liquidity
              </button>

              <div style={{borderTop:'1px solid var(--bdr)',paddingTop:12,fontWeight:600,fontSize:12,color:'var(--txl)'}}>Remove Liquidity</div>
              <div>
                <div style={{fontSize:11,color:'var(--txl)',marginBottom:4}}>LP tokens to burn</div>
                <input className="inp" type="number" placeholder="0" value={lpAmt} onChange={e=>setLpAmt(e.target.value)} style={{width:'100%',boxSizing:'border-box'}}/>
              </div>
              <button className="btn-o" style={{width:'100%',borderColor:'var(--red)',color:'var(--red)'}} onClick={handleRemoveLiq} disabled={busy||!wallet}>
                {busy?<RefreshCw size={14} className="spin"/>:<TrendingDown size={14}/>} Remove Liquidity
              </button>
            </div>
          )}

          {/* ── ALL POOLS ── */}
          {tab === 'pools' && (
            <div className="card fi" style={{display:'flex',flexDirection:'column',gap:8}}>
              <div style={{fontWeight:700,fontSize:14,marginBottom:4}}>All Pools</div>
              {pools.map(p=>(
                <div key={p.pool_id} onClick={()=>{setSelPool(p.pool_id);setTab('swap');}}
                  style={{background:'var(--bg3)',borderRadius:8,padding:'12px 14px',cursor:'pointer',border:`1px solid ${selPool===p.pool_id?'var(--purple)':'var(--bdr)'}`}}>
                  <div style={{display:'flex',justifyContent:'space-between',marginBottom:6}}>
                    <span style={{fontWeight:700}}>RF / {p.token_b}</span>
                    <span className="bdg bdg-p" style={{background:'#7c3aed22',color:'#a78bfa',border:'1px solid #7c3aed44'}}>AMM</span>
                  </div>
                  <div style={{display:'grid',gridTemplateColumns:'1fr 1fr',gap:4,fontSize:11,color:'var(--txl)'}}>
                    <div>Price: <span style={{color:'var(--green)'}}>{p.price?.toFixed(6)||'—'}</span></div>
                    <div>Volume: {fmtRF(p.volume_rf)}</div>
                    <div>Reserve RF: {fmtN(p.reserve_rf)}</div>
                    <div>Fees: {fmtRF(p.fees_collected)}</div>
                  </div>
                </div>
              ))}
            </div>
          )}

          {/* Bridge hint */}
          <div className="card fi" style={{border:'1px solid var(--bdr)',borderRadius:8,padding:'12px 14px',fontSize:12}}>
            <div style={{fontWeight:700,marginBottom:6,color:'var(--cyan)'}}>🌉 Cross-chain</div>
            <div style={{color:'var(--txl)',lineHeight:1.6}}>
              Haz bridge de ETH/BNB/MATIC/SOL/AVAX/ARB/BTC → wrapped tokens en redflag.web3,<br/>
              luego tradea en el DEX nativo con fees de solo 0.3%.
            </div>
            <div style={{display:'flex',gap:6,marginTop:8,flexWrap:'wrap'}}>
              {['Ethereum','BSC','Polygon','Solana','Avalanche','Arbitrum','Bitcoin'].map(c=><span key={c} className="bdg bdg-b">{c}</span>)}
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

// ─────────────────────────────────────────────
// BRIDGE PAGE
// ─────────────────────────────────────────────
const BRIDGE_CONTRACTS = {
  ETH:     { address: '0x92E83A72b3CD6d699cc8F16D756d5f31aCF55659', chainId: '0x1',  name: 'Ethereum', symbol: 'ETH',  explorer: 'https://etherscan.io/tx/' },
  BSC:     { address: '0x06436bf6E71964A99bD4078043aa4cDfA0eadEe6', chainId: '0x38', name: 'BNB Chain',  symbol: 'BNB',  explorer: 'https://bscscan.com/tx/' },
  Polygon: { address: '0x19D2A913a6df973a7ad600F420960235307c6Cbf', chainId: '0x89', name: 'Polygon',  symbol: 'MATIC', explorer: 'https://polygonscan.com/tx/' },
};
const BRIDGE_SOON = ['Solana (wSOL)', 'Avalanche (wAVAX)', 'Arbitrum (wARB)', 'Bitcoin (wBTC)'];

const LOCK_ABI = [{
  name: 'lock', type: 'function', stateMutability: 'payable',
  inputs: [{ name: 'rfAddress', type: 'string' }],
  outputs: [],
}];

function encodelock(rfAddress) {
  // ABI encode: lock(string rfAddress)
  // selector: keccak256("lock(string)")[0:4]
  const selector = '0xf83d08ba';
  const enc = new TextEncoder();
  const strBytes = enc.encode(rfAddress);
  const len = strBytes.length;
  // offset (32 bytes) + length (32 bytes) + data (padded to 32)
  const pad = n => n.toString(16).padStart(64, '0');
  const dataHex = Array.from(strBytes).map(b => b.toString(16).padStart(2,'0')).join('');
  const padded = dataHex.padEnd(Math.ceil(len / 32) * 64, '0');
  return selector + pad(32) + pad(len) + padded;
}

// ─────────────────────────────────────────────
// STAKING PAGE
// ─────────────────────────────────────────────
const RF_UNIT = 1_000_000; // 1 RF = 1,000,000 microRF
const MIN_STAKE_RF = 10_000; // mínimo 10,000 RF para ser validador

function StakingPage({ wallet, wsData }) {
  const [info, setInfo]       = useState(null);
  const [stakes, setStakes]   = useState([]);
  const [total, setTotal]     = useState(0);
  const [balance, setBalance] = useState(0);
  const [amount, setAmount]   = useState('');
  const [busy, setBusy]       = useState(false);
  const [msg, setMsg]         = useState(null);
  const [myStake, setMyStake] = useState(0);
  const [rewards, setRewards] = useState(0);

  const notify = (type, text) => { setMsg({ type, text }); setTimeout(() => setMsg(null), 5000); };

  const fetchData = useCallback(async () => {
    try {
      const [inf, st] = await Promise.all([sdk.getStakingInfo(), sdk.getStakes()]);
      setInfo(inf);
      setStakes(st.stakes || []);
      setTotal(st.total_staked || 0);
      if (wallet) {
        const bal = await sdk.getBalance(wallet.address);
        setBalance(bal.balance || 0);
        const mine = (st.stakes || []).find(s => s.address === wallet.address);
        setMyStake(mine?.staked_rf || 0);
      }
    } catch {}
  }, [wallet]);

  useEffect(() => { fetchData(); const iv = setInterval(fetchData, 30000); return () => clearInterval(iv); }, [fetchData]);
  useEffect(() => { if(wsData?.type==='new_block'||wsData?.type==='staking_stake') fetchData(); }, [wsData, fetchData]);

  // Estimar recompensas diarias: 1 RF + avg TXs por vértice, 5 vértices/seg
  useEffect(() => {
    if (myStake > 0 && stakes.length > 0) {
      // Estimación: 5 vértices/seg × 86400 seg / num_validators
      const daily = Math.floor((5 * 86400) / Math.max(stakes.length, 1));
      setRewards(daily);
    }
  }, [myStake, stakes]);

  const handleStake = async () => {
    if (!wallet) return notify('err', 'Conecta tu wallet primero');
    const amtRF = Number(amount);
    if (!amtRF || amtRF < MIN_STAKE_RF) return notify('err', `Mínimo ${MIN_STAKE_RF} RF para hacer stake`);
    const rawAmt = Math.floor(amtRF * RF_UNIT);
    if (balance < rawAmt) return notify('err', `Saldo insuficiente: tienes ${Math.floor(balance/RF_UNIT)} RF`);
    setBusy(true);
    try {
      const res = await sdk.stakingStake(wallet.private_key_hex, rawAmt);
      if (res.success) {
        notify('ok', `✅ ${amtRF} RF en stake — eres validador en la próxima ronda`);
        setAmount('');
        setTimeout(fetchData, 2000);
      } else notify('err', res.error || res.message || 'Error');
    } catch(e) { notify('err', e.response?.data?.error || e.message); }
    setBusy(false);
  };

  const handleUnstake = async () => {
    if (!wallet) return notify('err', 'Conecta tu wallet primero');
    if (myStake === 0) return notify('err', 'No tienes stake activo');
    setBusy(true);
    try {
      const res = await sdk.stakingUnstake(wallet.private_key_hex);
      if (res.success) notify('ok', res.message || '⏳ Unbonding iniciado — retira en 10 rondas');
      else notify('err', res.error || 'Error');
      setTimeout(fetchData, 2000);
    } catch(e) { notify('err', e.response?.data?.error || e.message); }
    setBusy(false);
  };

  const handleWithdraw = async () => {
    if (!wallet) return notify('err', 'Conecta tu wallet primero');
    setBusy(true);
    try {
      const res = await sdk.stakingWithdraw(wallet.private_key_hex);
      if (res.success) notify('ok', res.message || '✅ RF devueltos a tu wallet');
      else notify('err', res.error || 'Error');
      setTimeout(fetchData, 2000);
    } catch(e) { notify('err', e.response?.data?.error || e.message); }
    setBusy(false);
  };

  const pct = total > 0 && myStake > 0 ? (myStake / total * 100).toFixed(2) : 0;

  return (
    <div style={{display:'flex',flexDirection:'column',gap:16}}>
      <SectionHdr icon={Shield} title="Staking — Sé un Validador" color="var(--green)"
        right={<span className="bdg bdg-g" style={{fontSize:11}}>{stakes.length} validadores activos</span>}
      />
      {msg && <Alert type={msg.type}>{msg.text}</Alert>}

      {/* Stats top */}
      <div style={{display:'grid',gridTemplateColumns:'repeat(4,1fr)',gap:12}}>
        <StatCard icon={Shield}     label="Total Staked"     value={`${fmtN(Math.floor(total/1_000_000))} RF`} color="var(--green)"/>
        <StatCard icon={Users}      label="Validadores"      value={stakes.length} color="var(--cyan)"/>
        <StatCard icon={Zap}        label="Rondas/seg"       value="5" sub="200ms/ronda" color="var(--yellow)"/>
        <StatCard icon={TrendingUp} label="Mi stake"         value={myStake > 0 ? `${fmtN(Math.floor(myStake/1_000_000))} RF` : '—'} color="var(--purple)"/>
      </div>

      <div style={{display:'grid',gridTemplateColumns:'1fr 380px',gap:16}}>

        {/* Lista de validadores */}
        <div style={{display:'flex',flexDirection:'column',gap:12}}>
          <div className="card fi">
            <div style={{fontWeight:700,marginBottom:12,display:'flex',justifyContent:'space-between'}}>
              <span>Validadores activos</span>
              <span style={{fontSize:11,color:'var(--txl)'}}>{fmtN(Math.floor(total/1_000_000))} RF total stakeado</span>
            </div>
            <table className="tbl">
              <thead><tr><th>#</th><th>Dirección</th><th>Stake (RF)</th><th>% Red</th><th>Recompensas/día</th></tr></thead>
              <tbody>
                {stakes.map((s,i) => {
                  const p = total > 0 ? (s.staked_rf / total * 100).toFixed(1) : 0;
                  const dailyEst = Math.floor((5 * 86400) / Math.max(stakes.length, 1));
                  const isMe = wallet && s.address === wallet.address;
                  return (
                    <tr key={i} style={isMe ? {background:'var(--bg3)',outline:'1px solid var(--green)'} : {}}>
                      <td style={{color:'var(--txl)'}}>{i+1}</td>
                      <td style={{fontFamily:'monospace',fontSize:11}}>
                        {short(s.address, 12)}
                        {isMe && <span className="bdg bdg-g" style={{marginLeft:6,fontSize:9}}>YO</span>}
                      </td>
                      <td style={{fontWeight:700,color:'var(--green)'}}>{fmtN(Math.floor(s.staked_rf/1_000_000))}</td>
                      <td>
                        <div style={{display:'flex',alignItems:'center',gap:6}}>
                          <div style={{width:60,height:4,background:'var(--bg3)',borderRadius:2}}>
                            <div style={{width:`${p}%`,height:'100%',background:'var(--green)',borderRadius:2}}/>
                          </div>
                          <span style={{fontSize:11}}>{p}%</span>
                        </div>
                      </td>
                      <td style={{color:'var(--yellow)',fontWeight:700}}>~{fmtN(dailyEst)} RF</td>
                    </tr>
                  );
                })}
                {stakes.length === 0 && (
                  <tr><td colSpan={5} style={{textAlign:'center',padding:24,color:'var(--txl)'}}>
                    Sé el primero en hacer stake y convertirte en validador
                  </td></tr>
                )}
              </tbody>
            </table>
          </div>

          {/* Cómo funciona */}
          <div className="card fi" style={{fontSize:13,lineHeight:1.8}}>
            <div style={{fontWeight:700,marginBottom:8,color:'var(--cyan)'}}>¿Cómo funciona el staking?</div>
            <div style={{display:'flex',flexDirection:'column',gap:6,color:'var(--txl)'}}>
              {[
                ['1', 'Instala el nodo', 'curl -sSf https://redflagweb3-app.onrender.com/install.sh | bash'],
                ['2', 'Consigue tu dirección RF', 'Se genera automáticamente al iniciar el nodo'],
                ['3', 'Envía 10,000+ RF a STAKE_ADDRESS', 'Desde esta wallet o cualquier wallet RF'],
                ['4', 'Tu nodo se registra como validador', 'Automático en la siguiente ronda (200ms)'],
                ['5', 'Gana RF por cada bloque', '1 RF base + 0.1 RF por TX incluida'],
              ].map(([n,title,desc])=>(
                <div key={n} style={{display:'flex',gap:12,alignItems:'flex-start'}}>
                  <span style={{minWidth:22,height:22,borderRadius:'50%',background:'var(--green)',color:'#000',display:'flex',alignItems:'center',justifyContent:'center',fontWeight:700,fontSize:11}}>{n}</span>
                  <div>
                    <div style={{fontWeight:600,color:'var(--tx)'}}>{title}</div>
                    <div style={{fontSize:11,color:'var(--txl)',fontFamily:n==='3'||n==='1'?'monospace':'inherit'}}>{desc}</div>
                  </div>
                </div>
              ))}
            </div>
          </div>
        </div>

        {/* Panel stake */}
        <div style={{display:'flex',flexDirection:'column',gap:12}}>

          {/* Mi posición */}
          {myStake > 0 && (
            <div className="card fi" style={{border:'1px solid var(--green)'}}>
              <div style={{fontWeight:700,color:'var(--green)',marginBottom:10,display:'flex',alignItems:'center',gap:8}}>
                <CheckCircle2 size={16}/> Eres validador activo
              </div>
              <div style={{display:'grid',gridTemplateColumns:'1fr 1fr',gap:8,fontSize:12}}>
                <div style={{background:'var(--bg3)',borderRadius:8,padding:'10px 12px'}}>
                  <div style={{color:'var(--txl)',marginBottom:2}}>Mi stake</div>
                  <div style={{fontWeight:700,color:'var(--green)'}}>{fmtN(Math.floor(myStake/1_000_000))} RF</div>
                </div>
                <div style={{background:'var(--bg3)',borderRadius:8,padding:'10px 12px'}}>
                  <div style={{color:'var(--txl)',marginBottom:2}}>% de la red</div>
                  <div style={{fontWeight:700,color:'var(--cyan)'}}>{pct}%</div>
                </div>
                <div style={{background:'var(--bg3)',borderRadius:8,padding:'10px 12px'}}>
                  <div style={{color:'var(--txl)',marginBottom:2}}>Recompensas/día est.</div>
                  <div style={{fontWeight:700,color:'var(--yellow)'}}>~{fmtN(rewards)} RF</div>
                </div>
                <div style={{background:'var(--bg3)',borderRadius:8,padding:'10px 12px'}}>
                  <div style={{color:'var(--txl)',marginBottom:2}}>APY estimado</div>
                  <div style={{fontWeight:700,color:'var(--purple)'}}>
                    {myStake > 0 ? `~${((rewards * 365) / (myStake / 1_000_000) * 100).toFixed(0)}%` : '—'}
                  </div>
                </div>
              </div>
            </div>
          )}

          {/* Hacer stake */}
          <div className="card fi" style={{display:'flex',flexDirection:'column',gap:12}}>
            <div style={{fontWeight:700,fontSize:14,color:'var(--green)',display:'flex',alignItems:'center',gap:8}}>
              <Shield size={16}/> Hacer Stake
            </div>
            <div style={{fontSize:12,color:'var(--txl)'}}>
              Saldo disponible: <span style={{color:'var(--tx)',fontWeight:700}}>{fmtN(Math.floor(balance/RF_UNIT))} RF</span>
            </div>
            <div>
              <div style={{fontSize:11,color:'var(--txl)',marginBottom:4}}>Cantidad a stakear en RF (mín. {MIN_STAKE_RF.toLocaleString()} RF)</div>
              <input className="inp" type="number" placeholder="10000"
                value={amount} onChange={e=>setAmount(e.target.value)}
                style={{width:'100%',boxSizing:'border-box'}}
              />
            </div>

            {amount && Number(amount) >= MIN_STAKE_RF && (
              <div style={{background:'var(--bg3)',borderRadius:8,padding:'10px 12px',fontSize:12}}>
                <div style={{display:'flex',justifyContent:'space-between',marginBottom:4}}>
                  <span style={{color:'var(--txl)'}}>Recompensa/día estimada</span>
                  <span style={{fontWeight:700,color:'var(--yellow)'}}>~{fmtN(Math.floor(5*86400/Math.max(stakes.length+1,1)))} RF</span>
                </div>
                <div style={{display:'flex',justifyContent:'space-between'}}>
                  <span style={{color:'var(--txl)'}}>Registro como validador</span>
                  <span style={{color:'var(--green)'}}>Inmediato (siguiente ronda)</span>
                </div>
              </div>
            )}

            {!wallet && <Alert type="wrn">Conecta tu wallet para hacer stake</Alert>}
            <button className="btn-p" style={{background:'var(--green)',width:'100%'}} onClick={handleStake} disabled={busy||!wallet}>
              {busy ? <RefreshCw size={14} className="spin"/> : <Shield size={14}/>}
              {busy ? 'Procesando…' : 'Stake & Convertirme en Validador'}
            </button>
            {myStake > 0 && (
              <div style={{display:'flex',gap:8}}>
                <button className="btn" style={{flex:1,fontSize:12}} onClick={handleUnstake} disabled={busy||!wallet}>
                  ⏳ Iniciar Unstake
                </button>
                <button className="btn" style={{flex:1,fontSize:12,background:'var(--purple)'}} onClick={handleWithdraw} disabled={busy||!wallet}>
                  💰 Retirar RF
                </button>
              </div>
            )}
          </div>

          {/* Info económica */}
          <div className="card fi" style={{fontSize:12,color:'var(--txl)',lineHeight:1.7}}>
            <div style={{fontWeight:700,color:'var(--tx)',marginBottom:8}}>Economía del validador</div>
            <div>• <span style={{color:'var(--green)'}}>1 RF</span> base por cada vértice producido</div>
            <div>• <span style={{color:'var(--green)'}}>+0.1 RF</span> por cada TX incluida</div>
            <div>• Rondas cada <span style={{color:'var(--yellow)'}}>200ms</span> = 5 bloques/seg</div>
            <div>• ~<span style={{color:'var(--purple)'}}>{fmtN(5*86400)} vértices/día</span> por validador</div>
            <div style={{marginTop:8,paddingTop:8,borderTop:'1px solid var(--bdr)'}}>
              Cuantos más validadores haya, más se distribuyen los bloques y más segura y rápida es la red.
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

function BridgePage({ wallet }) {
  const [chain, setChain]     = useState('BSC');
  const [amount, setAmount]   = useState('');
  const [rfAddr, setRfAddr]   = useState(wallet?.address || '');
  const [status, setStatus]   = useState('');
  const [txHash, setTxHash]   = useState('');
  const [loading, setLoading] = useState(false);
  const [mmAddr, setMmAddr]   = useState('');

  const cfg = BRIDGE_CONTRACTS[chain];

  async function connectMeta() {
    if (!window.ethereum) { setStatus('MetaMask no detectado. Instálalo en metamask.io'); return; }
    try {
      const accounts = await window.ethereum.request({ method: 'eth_requestAccounts' });
      setMmAddr(accounts[0]);
      setStatus('MetaMask conectado: ' + accounts[0].slice(0,10) + '…');
    } catch (e) { setStatus('Error: ' + e.message); }
  }

  async function switchChain() {
    try {
      await window.ethereum.request({ method: 'wallet_switchEthereumChain', params: [{ chainId: cfg.chainId }] });
    } catch (e) {
      if (e.code === 4902) {
        const rpcMap = { '0x38': 'https://bsc-dataseed.binance.org', '0x89': 'https://polygon-rpc.com' };
        const nameMap = { '0x38': 'BNB Smart Chain', '0x89': 'Polygon Mainnet' };
        await window.ethereum.request({ method: 'wallet_addEthereumChain', params: [{
          chainId: cfg.chainId, chainName: nameMap[cfg.chainId] || cfg.name,
          rpcUrls: [rpcMap[cfg.chainId] || 'https://mainnet.infura.io/v3/'],
          nativeCurrency: { name: cfg.symbol, symbol: cfg.symbol, decimals: 18 },
        }]});
      }
    }
  }

  async function doBridge() {
    if (!window.ethereum)        { setStatus('Instala MetaMask'); return; }
    if (!mmAddr)                  { setStatus('Conecta MetaMask primero'); return; }
    if (!rfAddr || rfAddr.length < 16) { setStatus('Dirección RF inválida (muy corta)'); return; }
    const amtNum = parseFloat(amount);
    if (!amount || isNaN(amtNum) || amtNum <= 0) { setStatus('Introduce una cantidad válida'); return; }

    setLoading(true); setStatus('Cambiando de red…'); setTxHash('');
    try {
      await switchChain();
      const weiHex = '0x' + BigInt(Math.round(amtNum * 1e18)).toString(16);
      const data   = encodelock(rfAddr);
      setStatus('Confirmando en MetaMask…');
      const hash = await window.ethereum.request({
        method: 'eth_sendTransaction',
        params: [{ from: mmAddr, to: cfg.address, value: weiHex, data }],
      });
      setTxHash(hash);
      setStatus('✅ TX enviada. El bridge acreditará RF en ~1-2 min.');
    } catch (e) {
      setStatus('Error: ' + (e.message || JSON.stringify(e)));
    } finally { setLoading(false); }
  }

  return (
    <div style={{maxWidth:480,margin:'0 auto',display:'flex',flexDirection:'column',gap:16}}>
      <div className="card fi">
        <div style={{fontWeight:700,fontSize:16,marginBottom:4}}>🌉 Bridge → redflag.web3</div>
        <div style={{fontSize:12,color:'var(--txl)'}}>Bloquea ETH/BNB/MATIC y recibe RF en tu wallet</div>
      </div>

      {/* Chain selector */}
      <div className="card fi" style={{gap:12}}>
        <div style={{fontSize:12,color:'var(--txl)',fontWeight:600}}>RED ORIGEN</div>
        <div style={{display:'flex',gap:8}}>
          {Object.keys(BRIDGE_CONTRACTS).map(c=>(
            <button key={c} onClick={()=>setChain(c)}
              style={{flex:1,padding:'8px 4px',borderRadius:8,border:`1.5px solid ${chain===c?'var(--cyan)':'var(--bdr)'}`,
                background:chain===c?'rgba(0,220,255,.08)':'transparent',color:chain===c?'var(--cyan)':'var(--txl)',
                cursor:'pointer',fontSize:13,fontWeight:chain===c?700:400}}>
              {BRIDGE_CONTRACTS[c].symbol}
            </button>
          ))}
        </div>
        <div style={{fontSize:11,color:'var(--txl)'}}>
          Contrato: <span style={{fontFamily:'monospace'}}>{cfg.address.slice(0,18)}…</span>
        </div>
      </div>

      {/* MetaMask */}
      <div className="card fi" style={{gap:10}}>
        <div style={{fontSize:12,color:'var(--txl)',fontWeight:600}}>METAMASK</div>
        {mmAddr
          ? <div style={{fontSize:12,color:'var(--green)'}}>✅ {mmAddr.slice(0,16)}…{mmAddr.slice(-6)}</div>
          : <button className="btn" onClick={connectMeta} style={{width:'100%'}}>Conectar MetaMask</button>
        }
      </div>

      {/* Inputs */}
      <div className="card fi" style={{gap:12}}>
        <div style={{fontSize:12,color:'var(--txl)',fontWeight:600}}>CANTIDAD ({cfg.symbol})</div>
        <input className="inp" type="number" placeholder={`0.01 ${cfg.symbol}`} value={amount}
          onChange={e=>setAmount(e.target.value)} min="0" step="0.001"/>
        <div style={{fontSize:12,color:'var(--txl)',fontWeight:600}}>TU DIRECCIÓN RF (destino)</div>
        <input className="inp" placeholder="Tu dirección redflag.web3" value={rfAddr}
          onChange={e=>setRfAddr(e.target.value)} style={{fontFamily:'monospace',fontSize:11}}/>
        <div style={{fontSize:11,color:'var(--txl)'}}>
          Tu dirección RF: <span style={{fontFamily:'monospace',color:'var(--cyan)',cursor:'pointer'}}
            onClick={()=>setRfAddr(wallet?.address||'')}>{wallet?.address ? short(wallet.address,12) : '—'} (click para usar)</span>
        </div>
      </div>

      {/* Coming soon chains */}
      <div className="card fi" style={{background:'var(--bg3)'}}>
        <div style={{fontSize:12,color:'var(--txl)',fontWeight:600,marginBottom:8}}>PRÓXIMAMENTE</div>
        <div style={{display:'flex',gap:6,flexWrap:'wrap'}}>
          {BRIDGE_SOON.map(c=>(
            <span key={c} style={{padding:'4px 10px',borderRadius:6,background:'var(--bg)',border:'1px solid var(--bdr)',fontSize:11,color:'var(--txl)'}}>{c}</span>
          ))}
        </div>
      </div>

      {/* Send */}
      <button className="btn" onClick={doBridge} disabled={loading || !mmAddr}
        style={{width:'100%',padding:'12px',fontSize:15,opacity:loading||!mmAddr?0.5:1}}>
        {loading ? 'Procesando…' : `Enviar ${amount||'0'} ${cfg.symbol} → RF`}
      </button>

      {/* Status */}
      {status && (
        <div style={{background:'rgba(0,220,255,.06)',border:'1px solid var(--bdr)',borderRadius:8,padding:'10px 14px',fontSize:12,color:status.startsWith('✅')?'var(--green)':'var(--txl)'}}>
          {status}
          {txHash && (
            <div style={{marginTop:6}}>
              <a href={cfg.explorer + txHash} target="_blank" rel="noreferrer"
                style={{color:'var(--cyan)',fontSize:11}}>Ver en explorer →</a>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

// ─────────────────────────────────────────────
// GOVERNANCE PAGE
// ─────────────────────────────────────────────
function GovernancePage({wallet, wsData}) {
  const [proposals, setProposals] = useState([]);
  const [prices, setPrices] = useState([]);
  const [loading, setLoading] = useState(false);
  const [tab, setTab] = useState('proposals');
  const [form, setForm] = useState({title:'',description:'',param:'MinFee',new_value:''});
  const [msg, setMsg] = useState(null);
  const [currentRound, setCurrentRound] = useState(0);
  const [totalStaked, setTotalStaked] = useState(0);

  const load = useCallback(()=>{
    sdk.get('/governance/proposals').then(r=>setProposals(r.proposals||[])).catch(()=>{});
    sdk.get('/oracle/prices').then(r=>setPrices(r.prices||[])).catch(()=>{});
    sdk.getStatus().then(r=>setCurrentRound(r.current_round||0)).catch(()=>{});
    sdk.get('/staking/stakes').then(r=>setTotalStaked(r.total_staked||0)).catch(()=>{});
  },[]);
  useEffect(()=>{ load(); const id=setInterval(load,30000); return()=>clearInterval(id); },[load]);
  useEffect(()=>{ if(wsData?.type==='new_block') load(); },[wsData, load]);

  const propose = async(e)=>{
    e.preventDefault();
    if(!wallet?.private_key_hex) return setMsg({type:'err',text:'Conecta tu wallet primero'});
    setLoading(true); setMsg(null);
    try {
      const r = await sdk.post('/governance/propose',{
        private_key_hex: wallet.private_key_hex,
        title: form.title, description: form.description,
        param: form.param, new_value: parseInt(form.new_value)||0,
      });
      if(r.success) { setMsg({type:'ok',text:`Propuesta #${r.proposal_id} creada. Votación hasta ronda ${r.voting_end_round}`}); setForm({title:'',description:'',param:'MinFee',new_value:''}); load(); }
      else setMsg({type:'err',text:r.error});
    } catch(ex){ setMsg({type:'err',text:ex.message}); }
    finally{ setLoading(false); }
  };

  const vote = async(id, voteFor)=>{
    if(!wallet?.private_key_hex) return setMsg({type:'err',text:'Conecta tu wallet primero'});
    try {
      const r = await sdk.post('/governance/vote',{private_key_hex:wallet.private_key_hex, proposal_id:id, vote:voteFor});
      if(r.success) { setMsg({type:'ok',text:`Voto registrado (${r.stake_weight} RF de peso)`}); load(); }
      else setMsg({type:'err',text:r.error});
    } catch(ex){ setMsg({type:'err',text:ex.message}); }
  };

  const tabStyle=(t)=>({padding:'8px 18px',cursor:'pointer',fontSize:13,fontWeight:600,
    borderBottom:tab===t?'2px solid var(--cyan)':'2px solid transparent',
    color:tab===t?'var(--cyan)':'var(--txm)',background:'none',border:'none',
    borderBottomWidth:2,borderBottomStyle:'solid'});

  const statusColor={active:'var(--cyan)',passed:'var(--green)',rejected:'var(--red)',pending_finalization:'var(--yellow)'};

  return (
    <div>
      <div style={{display:'flex',borderBottom:'1px solid var(--border)',marginBottom:20}}>
        <button style={tabStyle('proposals')} onClick={()=>setTab('proposals')}>Proposals ({proposals.length})</button>
        <button style={tabStyle('propose')} onClick={()=>setTab('propose')}>New Proposal</button>
        <button style={tabStyle('oracle')} onClick={()=>setTab('oracle')}>Price Oracle</button>
      </div>

      {msg && <Alert type={msg.type==='ok'?'ok':'err'} style={{marginBottom:16}}>{msg.text}</Alert>}

      {tab==='proposals' && (
        <div>
          {proposals.length===0 && <div style={{textAlign:'center',color:'var(--txl)',padding:40}}>No hay propuestas aún. Sé el primero en crear una.</div>}
          {proposals.map(p=>{
            const totalVotes = (p.votes_for||0)+(p.votes_against||0);
            const yesPercent = totalVotes>0 ? p.votes_for/totalVotes*100 : 0;
            const quorumPercent = totalStaked>0 ? totalVotes/totalStaked*100 : 0;
            const roundsLeft = p.status==='active' ? Math.max(0,(p.voting_end_round||0)-currentRound) : 0;
            const alreadyVoted = wallet && p.voters?.includes(wallet.address);
            return (
            <div key={p.id} className="card fi" style={{marginBottom:16}}>
              <div style={{display:'flex',alignItems:'center',gap:10,marginBottom:8,flexWrap:'wrap'}}>
                <span style={{fontFamily:'var(--acc)',fontWeight:700,fontSize:15}}>#{p.id} {p.title}</span>
                <span style={{fontSize:11,padding:'2px 8px',borderRadius:4,background:'rgba(255,255,255,0.07)',color:statusColor[p.status]||'var(--txm)',fontWeight:700}}>{p.status}</span>
                {p.status==='active' && roundsLeft>0 && (
                  <span style={{fontSize:11,color:'var(--yellow)',display:'flex',alignItems:'center',gap:4}}>
                    <Clock size={11}/> {roundsLeft} rounds left
                  </span>
                )}
                {alreadyVoted && <span style={{fontSize:11,color:'var(--green)'}}>✓ Voted</span>}
              </div>
              <div style={{fontSize:13,color:'var(--txm)',marginBottom:10}}>{p.description}</div>
              <div style={{display:'flex',gap:16,fontSize:12,color:'var(--txl)',marginBottom:10,flexWrap:'wrap'}}>
                <span>Param: <b style={{color:'var(--txt)'}}>{p.param}</b></span>
                <span>→ <b style={{color:'var(--cyan)'}}>{p.new_value}</b></span>
                <span>Proposer: <b>{short(p.proposer||'',12)}</b></span>
                <span>Voters: <b>{p.voter_count||0}</b></span>
              </div>

              {/* Vote bars */}
              <div style={{display:'flex',flexDirection:'column',gap:6,marginBottom:10}}>
                <div style={{display:'flex',alignItems:'center',gap:10}}>
                  <span style={{fontSize:11,color:'var(--green)',minWidth:28}}>SÍ</span>
                  <div style={{flex:1,background:'var(--bg)',borderRadius:4,height:6,overflow:'hidden'}}>
                    <div style={{width:`${yesPercent}%`,height:'100%',background:'var(--green)',transition:'width .3s'}}/>
                  </div>
                  <span style={{fontSize:11,color:'var(--green)',minWidth:60,textAlign:'right'}}>{fmtN(p.votes_for||0)} RF</span>
                </div>
                <div style={{display:'flex',alignItems:'center',gap:10}}>
                  <span style={{fontSize:11,color:'var(--red)',minWidth:28}}>NO</span>
                  <div style={{flex:1,background:'var(--bg)',borderRadius:4,height:6,overflow:'hidden'}}>
                    <div style={{width:`${100-yesPercent}%`,height:'100%',background:'var(--red)',transition:'width .3s'}}/>
                  </div>
                  <span style={{fontSize:11,color:'var(--red)',minWidth:60,textAlign:'right'}}>{fmtN(p.votes_against||0)} RF</span>
                </div>
                {/* Quorum bar */}
                <div style={{display:'flex',alignItems:'center',gap:10}}>
                  <span style={{fontSize:10,color:'var(--txl)',minWidth:28}}>QRM</span>
                  <div style={{flex:1,background:'var(--bg)',borderRadius:4,height:4,overflow:'hidden'}}>
                    <div style={{width:`${Math.min(100,quorumPercent)}%`,height:'100%',background:quorumPercent>=10?'var(--cyan)':'var(--yellow)',transition:'width .3s'}}/>
                  </div>
                  <span style={{fontSize:10,color:quorumPercent>=10?'var(--cyan)':'var(--yellow)',minWidth:60,textAlign:'right'}}>{quorumPercent.toFixed(1)}% / 10%</span>
                </div>
              </div>

              {p.status==='active' && !alreadyVoted && (
                <div style={{display:'flex',gap:8}}>
                  <button className="btn btn-p" style={{flex:1,fontSize:12}} onClick={()=>vote(p.id,true)}>✓ Votar SÍ</button>
                  <button className="btn" style={{flex:1,fontSize:12,borderColor:'var(--red)',color:'var(--red)'}} onClick={()=>vote(p.id,false)}>✗ Votar NO</button>
                </div>
              )}
              {p.status==='active' && alreadyVoted && (
                <div style={{fontSize:12,color:'var(--green)',textAlign:'center',padding:'6px 0'}}>✓ Tu voto ha sido registrado</div>
              )}
            </div>
            );
          })}
        </div>
      )}

      {tab==='propose' && (
        <div className="card fi">
          <SectionHdr icon={Users} title="Nueva Propuesta" color="var(--cyan)"/>
          <form onSubmit={propose} style={{display:'flex',flexDirection:'column',gap:14,marginTop:16}}>
            <input className="inp" placeholder="Título" value={form.title} onChange={e=>setForm({...form,title:e.target.value})} required/>
            <textarea className="inp" placeholder="Descripción" rows={3} value={form.description} onChange={e=>setForm({...form,description:e.target.value})} style={{resize:'vertical'}} required/>
            <div style={{display:'flex',gap:12}}>
              <select className="inp" value={form.param} onChange={e=>setForm({...form,param:e.target.value})} style={{flex:1}}>
                {['MinFee','HalvingInterval','MinStake','FeeBurnPercent','UnstakeDelay'].map(p=>(
                  <option key={p} value={p}>{p}</option>
                ))}
              </select>
              <input className="inp" placeholder="Nuevo valor" type="number" value={form.new_value} onChange={e=>setForm({...form,new_value:e.target.value})} style={{flex:1}} required/>
            </div>
            <button className="btn btn-p" type="submit" disabled={loading||!wallet?.private_key_hex}>
              {loading?<span className="spin"><RefreshCw size={14}/></span>:<Users size={14}/>} Crear Propuesta
            </button>
            {!wallet?.private_key_hex && <Alert type="wrn">Necesitas wallet conectada para proponer</Alert>}
          </form>
        </div>
      )}

      {tab==='oracle' && (
        <div>
          <div className="card fi">
            <SectionHdr icon={TrendingUp} title="Price Oracle — Mediana de validadores" color="var(--purple)"/>
            <div style={{marginTop:16,display:'grid',gridTemplateColumns:'repeat(auto-fill,minmax(200px,1fr))',gap:12}}>
              {prices.length===0 && <div style={{color:'var(--txl)',fontSize:13}}>Sin datos de precio aún. Los validadores deben enviar precios.</div>}
              {prices.map(p=>(
                <div key={p.pair} style={{background:'var(--bg)',borderRadius:8,padding:14}}>
                  <div style={{fontSize:12,color:'var(--txl)',marginBottom:4}}>{p.pair}</div>
                  <div style={{fontSize:22,fontWeight:700,fontFamily:'var(--acc)',color:'var(--cyan)'}}>
                    ${p.price_usd?.toFixed(6)||'—'}
                  </div>
                  <div style={{fontSize:11,color:'var(--txl)',marginTop:4}}>{p.submissions} validadores · ronda {p.last_updated_round}</div>
                </div>
              ))}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}

// ─────────────────────────────────────────────
// MONITORING PAGE
// ─────────────────────────────────────────────
const NODES = [
  { name:'node1', url:'https://redflagweb3-node1.onrender.com', committee:true },
  { name:'node2', url:'https://redflagweb3-node2.onrender.com', committee:true },
  { name:'node3', url:'https://redflagweb3-node3.onrender.com', committee:true },
  { name:'node4', url:'https://redflagweb3-node4.onrender.com', committee:false },
  { name:'node5', url:'https://redflagweb3-node5.onrender.com', committee:false },
];

function MonitoringPage() {
  const [nodes, setNodes] = useState(NODES.map(n=>({...n,status:'checking',round:0,validators:0,uptime:0,version:'—'})));
  const [last, setLast] = useState(null);

  const checkNodes = useCallback(async()=>{
    const results = await Promise.all(NODES.map(async n => {
      try {
        const [st, nw] = await Promise.all([
          fetch(`${n.url}/status`, {signal:AbortSignal.timeout(5000)}).then(r=>r.json()),
          fetch(`${n.url}/network/stats`, {signal:AbortSignal.timeout(5000)}).then(r=>r.json()),
        ]);
        return {
          ...n, status:'online',
          round: st.current_round||0,
          validators: st.validator_count||0,
          pending: st.pending_txs||0,
          uptime: nw.node?.uptime_secs||0,
          version: st.version||'—',
          peer_id: st.peer_id ? st.peer_id.slice(0,16)+'…' : '—',
        };
      } catch {
        return {...n, status:'offline', round:0, validators:0, pending:0, uptime:0, version:'—', peer_id:'—'};
      }
    }));
    setNodes(results);
    setLast(new Date().toLocaleTimeString());
  },[]);

  useEffect(()=>{ checkNodes(); const iv=setInterval(checkNodes,15000); return()=>clearInterval(iv); },[checkNodes]);

  const online = nodes.filter(n=>n.status==='online').length;
  const maxRound = Math.max(...nodes.map(n=>n.round));

  return (
    <div style={{display:'flex',flexDirection:'column',gap:16}}>
      <div style={{display:'grid',gridTemplateColumns:'repeat(auto-fill,minmax(140px,1fr))',gap:12}}>
        <StatCard icon={Activity} label="Nodes Online" value={`${online}/${NODES.length}`} color={online>=3?'var(--green)':'var(--red)'}/>
        <StatCard icon={Cpu} label="Max Round" value={fmtN(maxRound)} color="var(--cyan)"/>
        <StatCard icon={Shield} label="BFT Quorum" value={online>=3?'Active':'Degraded'} color={online>=3?'var(--green)':'var(--red)'}/>
        <StatCard icon={Clock} label="Last Check" value={last||'—'} color="var(--txm)"/>
      </div>

      <div className="card fi">
        <SectionHdr icon={Activity} title="Node Health" right={<button className="ibtn" onClick={checkNodes} title="Refresh"><RefreshCw size={13}/></button>}/>
        <div style={{marginTop:16,display:'flex',flexDirection:'column',gap:10}}>
          {nodes.map(n=>(
            <div key={n.name} style={{display:'flex',alignItems:'center',gap:12,padding:'10px 14px',background:'var(--bg)',borderRadius:8,flexWrap:'wrap'}}>
              <div style={{display:'flex',alignItems:'center',gap:8,minWidth:80}}>
                <div className={`ldot ${n.status==='online'?'on':'off'}`}/>
                <span style={{fontWeight:700,fontSize:13}}>{n.name}</span>
              </div>
              <span style={{fontSize:11,color:n.status==='online'?'var(--green)':'var(--red)',fontWeight:600,minWidth:55}}>{n.status}</span>
              <div style={{flex:1,display:'flex',gap:16,flexWrap:'wrap',fontSize:12,color:'var(--txm)'}}>
                <span>Round: <b style={{color:'var(--fg)'}}>{fmtN(n.round)}</b></span>
                <span>Validators: <b style={{color:'var(--fg)'}}>{n.validators}</b></span>
                <span>Pending: <b style={{color:'var(--fg)'}}>{n.pending||0}</b></span>
                <span>Uptime: <b style={{color:'var(--fg)'}}>{n.uptime ? Math.floor(n.uptime/3600)+'h '+Math.floor((n.uptime%3600)/60)+'m' : '—'}</b></span>
                <span>v{n.version}</span>
              </div>
              {n.round>0 && maxRound>0 && n.round < maxRound-5 && (
                <span style={{fontSize:11,color:'var(--yellow)',fontWeight:600}}>⚠ Lagging {maxRound-n.round} rounds</span>
              )}
            </div>
          ))}
        </div>
      </div>

      <div className="card fi">
        <SectionHdr icon={Globe} title="Network Endpoints"/>
        <div style={{marginTop:12,display:'flex',flexDirection:'column',gap:6}}>
          {NODES.map(n=>(
            <div key={n.name} style={{display:'flex',justifyContent:'space-between',alignItems:'center',fontSize:12,padding:'6px 0',borderBottom:'1px solid var(--bdr)'}}>
              <span style={{color:'var(--txm)'}}>{n.name}</span>
              <div style={{display:'flex',alignItems:'center',gap:6}}>
                <span style={{fontFamily:'monospace',color:'var(--cyan)'}}>{n.url}</span>
                <CopyBtn text={n.url}/>
              </div>
            </div>
          ))}
        </div>
      </div>

      <div className="card fi">
        <SectionHdr icon={Shield} title="Security Status"/>
        <div style={{marginTop:12,display:'grid',gridTemplateColumns:'repeat(auto-fill,minmax(200px,1fr))',gap:10}}>
          {[
            {label:'Consensus', value:'Bullshark DAG BFT', ok:true},
            {label:'Signatures', value:'ML-DSA-65 (Post-Quantum)', ok:true},
            {label:'Encryption', value:'ML-KEM-768 (Post-Quantum)', ok:true},
            {label:'Bridge Mode', value:'Threshold Multi-Sig 2-of-3', ok:true},
            {label:'Persistent Storage', value:'Render Disks (1GB/node)', ok:true},
            {label:'State Sync', value:'Active (node2–node5)', ok:true},
          ].map(item=>(
            <div key={item.label} style={{padding:'10px 12px',background:'var(--bg)',borderRadius:8}}>
              <div style={{fontSize:11,color:'var(--txl)',marginBottom:4}}>{item.label}</div>
              <div style={{fontSize:12,fontWeight:600,color:item.ok?'var(--green)':'var(--red)',display:'flex',gap:6,alignItems:'center'}}>
                {item.ok ? <CheckCircle2 size={12}/> : <AlertTriangle size={12}/>}
                {item.value}
              </div>
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

// ─────────────────────────────────────────────
// DOCS PAGE
// ─────────────────────────────────────────────
function DocsPage() {
  const [section, setSection] = useState('overview');
  const sections = [
    {id:'overview',    label:'Overview'},
    {id:'wallet',      label:'Wallet & Keys'},
    {id:'send',        label:'Send & Receive'},
    {id:'staking',     label:'Staking'},
    {id:'dex',         label:'DEX Trading'},
    {id:'bridge',      label:'Bridge'},
    {id:'governance',  label:'Governance'},
    {id:'api',         label:'API Reference'},
    {id:'security',    label:'Security'},
  ];
  return (
    <div style={{display:'flex',gap:16,alignItems:'flex-start'}}>
      <div className="card fi" style={{minWidth:160,position:'sticky',top:16}}>
        <div style={{fontWeight:700,fontSize:12,color:'var(--txl)',marginBottom:10,letterSpacing:1}}>CONTENTS</div>
        {sections.map(s=>(
          <div key={s.id} onClick={()=>setSection(s.id)}
            style={{padding:'7px 10px',borderRadius:6,cursor:'pointer',fontSize:13,
              background:section===s.id?'var(--red)14':'transparent',
              color:section===s.id?'var(--red)':'var(--txm)',fontWeight:section===s.id?700:400}}>
            {s.label}
          </div>
        ))}
      </div>
      <div style={{flex:1,display:'flex',flexDirection:'column',gap:16}}>
        {section==='overview' && (
          <div className="card fi">
            <h2 style={{fontFamily:'var(--acc)',fontSize:20,marginBottom:12}}>redflag.web3</h2>
            <p style={{color:'var(--txm)',lineHeight:1.7,marginBottom:12}}>
              RedFlag is a post-quantum blockchain using <b style={{color:'var(--fg)'}}>ML-DSA-65</b> signatures and <b style={{color:'var(--fg)'}}>ML-KEM-768</b> encryption — resistant to quantum computing attacks. It uses <b style={{color:'var(--fg)'}}>Bullshark DAG</b> BFT consensus for fast finality.
            </p>
            <div style={{display:'flex',gap:10,marginBottom:14,flexWrap:'wrap'}}>
              {[
                {label:'𝕏 Twitter',  href:'https://x.com/franff546758',          color:'var(--fg)'},
                {label:'Telegram',   href:'https://t.me/redflag21blockchain',      color:'var(--cyan)'},
                {label:'GitHub',     href:'https://github.com/franklin0000/redflagweb3', color:'var(--txm)'},
              ].map(l=>(
                <a key={l.label} href={l.href} target="_blank" rel="noreferrer"
                  style={{display:'inline-flex',alignItems:'center',gap:6,padding:'6px 12px',
                    borderRadius:20,background:'var(--bg)',border:'1px solid var(--bdr)',
                    color:l.color,fontSize:12,textDecoration:'none',fontWeight:600}}>
                  {l.label}
                </a>
              ))}
            </div>
            <div style={{display:'grid',gridTemplateColumns:'repeat(auto-fill,minmax(160px,1fr))',gap:10,marginTop:8}}>
              {[
                {label:'Chain ID',     value:'2100'},
                {label:'Consensus',    value:'Bullshark DAG BFT'},
                {label:'Block Time',   value:'~200ms'},
                {label:'Total Supply', value:'1.5B RF'},
                {label:'Min Fee',      value:'1 RF'},
                {label:'Min Stake',    value:'10,000 RF'},
              ].map(i=>(
                <div key={i.label} style={{padding:'10px 12px',background:'var(--bg)',borderRadius:8}}>
                  <div style={{fontSize:11,color:'var(--txl)'}}>{i.label}</div>
                  <div style={{fontSize:14,fontWeight:700,marginTop:4}}>{i.value}</div>
                </div>
              ))}
            </div>
          </div>
        )}
        {section==='wallet' && (
          <div className="card fi">
            <h2 style={{fontFamily:'var(--acc)',fontSize:18,marginBottom:12}}>Wallet & Keys</h2>
            <div style={{display:'flex',flexDirection:'column',gap:14,color:'var(--txm)',lineHeight:1.7}}>
              <div><b style={{color:'var(--fg)'}}>Key Type:</b> ML-DSA-65 (FIPS 204) — post-quantum signature scheme. Your address IS your public key (hex-encoded).</div>
              <div><b style={{color:'var(--fg)'}}>Private Key:</b> Stored encrypted (AES-256-GCM) in your browser with your password. Never sent to any server.</div>
              <div><b style={{color:'var(--fg)'}}>Recovery:</b> Export your wallet from Settings to get the encrypted keystore JSON. Save your mnemonic (24 words) as backup.</div>
              <div style={{padding:'10px 14px',background:'var(--red)10',borderRadius:8,border:'1px solid var(--red)30'}}>
                <b style={{color:'var(--red)'}}>⚠ Security:</b> Never share your private key or mnemonic. The team will never ask for them.
              </div>
            </div>
          </div>
        )}
        {section==='send' && (
          <div className="card fi">
            <h2 style={{fontFamily:'var(--acc)',fontSize:18,marginBottom:12}}>Send & Receive RF</h2>
            <div style={{display:'flex',flexDirection:'column',gap:12,color:'var(--txm)',lineHeight:1.7}}>
              <div><b style={{color:'var(--fg)'}}>To receive:</b> Share your address (public key). It starts with a long hex string.</div>
              <div><b style={{color:'var(--fg)'}}>To send:</b> Go to Wallet → Send. Enter recipient address, amount (min 1 RF), and fee (default 1 RF).</div>
              <div><b style={{color:'var(--fg)'}}>Fees:</b> All fees go to the protocol fee pool, distributed to validators proportionally to their stake.</div>
              <div><b style={{color:'var(--fg)'}}>Faucet:</b> Get 1,000 RF free from the faucet (once per 24h) to get started.</div>
              <div><b style={{color:'var(--fg)'}}>Finality:</b> Transactions are finalized in the next committed DAG vertex (~200ms).</div>
            </div>
          </div>
        )}
        {section==='staking' && (
          <div className="card fi">
            <h2 style={{fontFamily:'var(--acc)',fontSize:18,marginBottom:12}}>Staking</h2>
            <div style={{display:'flex',flexDirection:'column',gap:12,color:'var(--txm)',lineHeight:1.7}}>
              <div><b style={{color:'var(--fg)'}}>Minimum stake:</b> 10,000 RF to become a validator.</div>
              <div><b style={{color:'var(--fg)'}}>Rewards:</b> 1 RF per vertex + 0.1 RF per TX included. Plus proportional share of protocol fees.</div>
              <div><b style={{color:'var(--fg)'}}>Unstake:</b> 10-round unbonding period (~2 seconds). After that, withdraw your RF back to your wallet.</div>
              <div><b style={{color:'var(--fg)'}}>Slashing:</b> Malicious validators can be slashed (stake reduced) by governance vote.</div>
              <div style={{padding:'10px 14px',background:'var(--cyan)10',borderRadius:8,border:'1px solid var(--cyan)30'}}>
                <b style={{color:'var(--cyan)'}}>Tip:</b> Run your own node for maximum rewards. See the Node Setup guide.
              </div>
            </div>
          </div>
        )}
        {section==='dex' && (
          <div className="card fi">
            <h2 style={{fontFamily:'var(--acc)',fontSize:18,marginBottom:12}}>DEX Trading</h2>
            <div style={{display:'flex',flexDirection:'column',gap:12,color:'var(--txm)',lineHeight:1.7}}>
              <div><b style={{color:'var(--fg)'}}>AMM:</b> Constant product (x·y=k), same model as Uniswap V2.</div>
              <div><b style={{color:'var(--fg)'}}>Pairs:</b> All pairs are RF ↔ wrapped token (wETH, wBNB, wMATIC, wSOL, wAVAX, wARB, wBTC, wUSDC, wUSDT).</div>
              <div><b style={{color:'var(--fg)'}}>Fee:</b> 0.3% per swap (30 bps), goes to the DEX fee pool then redistributed to LPs.</div>
              <div><b style={{color:'var(--fg)'}}>Liquidity:</b> Provide liquidity to earn LP tokens and a share of swap fees.</div>
              <div><b style={{color:'var(--fg)'}}>Slippage:</b> Set min_amount_out when swapping to protect against price movement.</div>
            </div>
          </div>
        )}
        {section==='bridge' && (
          <div className="card fi">
            <h2 style={{fontFamily:'var(--acc)',fontSize:18,marginBottom:12}}>Bridge</h2>
            <div style={{display:'flex',flexDirection:'column',gap:12,color:'var(--txm)',lineHeight:1.7}}>
              <div><b style={{color:'var(--fg)'}}>EVM → RF:</b> Lock ETH/BNB/MATIC in the bridge contract → bridge mints wETH/wBNB/wMATIC on redflag.web3.</div>
              <div><b style={{color:'var(--fg)'}}>RF → EVM:</b> Burn wrapped tokens on RF → bridge releases native tokens on EVM.</div>
              <div><b style={{color:'var(--fg)'}}>Security:</b> Threshold multi-sig: 2-of-3 nodes must approve each mint (ML-DSA signatures).</div>
              <div><b style={{color:'var(--fg)'}}>Supported chains:</b> Ethereum, BSC, Polygon. More coming: Solana, Avalanche, Arbitrum.</div>
              <div style={{display:'flex',flexDirection:'column',gap:6,marginTop:4}}>
                {[
                  {chain:'Ethereum', contract:'0x92E83A72b3CD6d699cc8F16D756d5f31aCF55659'},
                  {chain:'BSC',      contract:'0x06436bf6E71964A99bD4078043aa4cDfA0eadEe6'},
                  {chain:'Polygon',  contract:'0x19D2A913a6df973a7ad600F420960235307c6Cbf'},
                ].map(b=>(
                  <div key={b.chain} style={{display:'flex',justifyContent:'space-between',alignItems:'center',fontSize:12,padding:'6px 10px',background:'var(--bg)',borderRadius:6}}>
                    <span style={{fontWeight:600}}>{b.chain}</span>
                    <div style={{display:'flex',gap:6,alignItems:'center'}}>
                      <code style={{fontSize:11,color:'var(--cyan)'}}>{b.contract}</code>
                      <CopyBtn text={b.contract}/>
                    </div>
                  </div>
                ))}
              </div>
            </div>
          </div>
        )}
        {section==='governance' && (
          <div className="card fi">
            <h2 style={{fontFamily:'var(--acc)',fontSize:18,marginBottom:12}}>Governance</h2>
            <div style={{display:'flex',flexDirection:'column',gap:12,color:'var(--txm)',lineHeight:1.7}}>
              <div><b style={{color:'var(--fg)'}}>Voting power:</b> 1 RF staked = 1 vote. Must have active stake to propose or vote.</div>
              <div><b style={{color:'var(--fg)'}}>Quorum:</b> 10% of total staked RF must participate for a proposal to pass.</div>
              <div><b style={{color:'var(--fg)'}}>Voting period:</b> 100 rounds (~20 seconds). Vote before it expires.</div>
              <div><b style={{color:'var(--fg)'}}>Governable params:</b> MinFee, MinStake, HalvingInterval, FeeBurnPercent, UnstakeDelay.</div>
              <div><b style={{color:'var(--fg)'}}>Execution:</b> Passed proposals are executed automatically at round end.</div>
            </div>
          </div>
        )}
        {section==='api' && (
          <div className="card fi">
            <h2 style={{fontFamily:'var(--acc)',fontSize:18,marginBottom:12}}>API Reference</h2>
            <div style={{display:'flex',flexDirection:'column',gap:8}}>
              {[
                {method:'GET',  path:'/status',               desc:'Node status (round, validators, mempool)'},
                {method:'GET',  path:'/network/stats',        desc:'Full network statistics'},
                {method:'GET',  path:'/balance/:address',     desc:'RF balance and nonce'},
                {method:'GET',  path:'/history/:address',     desc:'Transaction history'},
                {method:'POST', path:'/wallet/send',          desc:'Sign and submit a transaction'},
                {method:'POST', path:'/wallet/faucet',        desc:'Request test RF (once/24h)'},
                {method:'GET',  path:'/dex/pools',            desc:'List all DEX pools'},
                {method:'POST', path:'/dex/swap',             desc:'Execute a swap'},
                {method:'POST', path:'/dex/quote',            desc:'Get swap quote (no execution)'},
                {method:'GET',  path:'/explorer/search/:q',  desc:'Search address, tx hash, vertex'},
                {method:'GET',  path:'/staking/stakes',       desc:'All active validators'},
                {method:'POST', path:'/staking/stake',        desc:'Stake RF to become validator'},
                {method:'GET',  path:'/api/v1/ticker',        desc:'CoinGecko-compatible ticker'},
                {method:'GET',  path:'/metrics',              desc:'Prometheus metrics'},
                {method:'WS',   path:'/ws',                   desc:'Real-time events (new_block, new_tx, dex_swap…)'},
              ].map(e=>(
                <div key={e.path} style={{display:'flex',gap:10,alignItems:'flex-start',padding:'7px 0',borderBottom:'1px solid var(--bdr)',fontSize:12}}>
                  <span style={{
                    minWidth:45,fontWeight:700,fontSize:10,padding:'2px 6px',borderRadius:4,
                    background:e.method==='GET'?'var(--cyan)20':e.method==='POST'?'var(--green)20':'var(--purple)20',
                    color:e.method==='GET'?'var(--cyan)':e.method==='POST'?'var(--green)':'var(--purple)',
                  }}>{e.method}</span>
                  <code style={{color:'var(--fg)',minWidth:220,fontSize:11}}>{e.path}</code>
                  <span style={{color:'var(--txm)'}}>{e.desc}</span>
                </div>
              ))}
            </div>
            <div style={{marginTop:16,padding:'10px 14px',background:'var(--bg)',borderRadius:8,fontSize:12}}>
              <b>Base URLs:</b>
              <div style={{fontFamily:'monospace',marginTop:6,color:'var(--cyan)'}}>https://redflagweb3-node1.onrender.com</div>
            </div>
          </div>
        )}
        {section==='security' && (
          <div className="card fi">
            <h2 style={{fontFamily:'var(--acc)',fontSize:18,marginBottom:12}}>Security</h2>
            <div style={{display:'flex',flexDirection:'column',gap:12,color:'var(--txm)',lineHeight:1.7}}>
              <div><b style={{color:'var(--fg)'}}>Post-Quantum:</b> ML-DSA-65 (FIPS 204) for signatures, ML-KEM-768 (FIPS 203) for threshold encryption. Quantum-resistant from day one.</div>
              <div><b style={{color:'var(--fg)'}}>BFT Safety:</b> Bullshark DAG requires 2f+1 honest nodes (f = faulty). With 3 nodes, tolerates 1 Byzantine failure.</div>
              <div><b style={{color:'var(--fg)'}}>Bridge security:</b> Threshold 2-of-3 multi-sig. Compromising 1 node is not enough to forge a mint.</div>
              <div><b style={{color:'var(--fg)'}}>Rate limits:</b> 60 req/min per IP. Faucet: 24h cooldown per address.</div>
              <div><b style={{color:'var(--fg)'}}>Supply cap:</b> Max 1 quadrillion units per token to prevent overflow exploits.</div>
              <div><b style={{color:'var(--fg)'}}>Nonce protection:</b> Replay attacks prevented by strict nonce ordering.</div>
              <div style={{padding:'10px 14px',background:'var(--cyan)10',borderRadius:8,border:'1px solid var(--cyan)30',marginTop:4}}>
                Found a vulnerability? Report responsibly —{' '}
                <a href="https://t.me/redflag21blockchain" target="_blank" rel="noreferrer"
                  style={{color:'var(--cyan)',fontWeight:600}}>join our Telegram</a> or open a GitHub issue.
              </div>
            </div>
          </div>
        )}
      </div>
    </div>
  );
}

// ─────────────────────────────────────────────
const PAGES = [
  {id:'wallet',     label:'Wallet',      icon:Wallet},
  {id:'dashboard',  label:'Dashboard',   icon:Activity},
  {id:'staking',    label:'Staking',     icon:Shield},
  {id:'bridge',     label:'Bridge',      icon:ArrowLeftRight},
  {id:'dex',        label:'DEX Trading', icon:TrendingUp},
  {id:'explorer',   label:'Explorer',    icon:Search},
  {id:'governance', label:'Governance',  icon:Users},
  {id:'monitor',    label:'Monitoring',  icon:Cpu},
  {id:'docs',       label:'Docs',        icon:FileCode},
  {id:'network',    label:'Network',     icon:Globe},
  {id:'settings',   label:'Settings',    icon:Settings},
];

export default function App() {
  const [wallet,setWallet]   = useState(null);
  const [page,setPage]       = useState('dashboard');
  const [online,setOnline]   = useState(false);
  const [stats,setStats]     = useState({});
  const [vertices,setVertices] = useState([]);
  const [mempool,setMempool] = useState({count:0,txs:[]});
  const [roundEk,setRoundEk] = useState({round:0,ek_hex:''});
  const [tpsHist,setTpsHist] = useState([]);
  const [pendingBadge,setPending] = useState(0);
  const [wsData,setWsData]   = useState(null);
  const [search,setSearch]   = useState('');
  const [searchQuery,setSearchQuery] = useState('');
  const [sidebarOpen,setSidebarOpen] = useState(false);
  const [activeProposals,setActiveProposals] = useState(0);
  const prevTx = useRef(0);

  const ks = hasKeystore();
  const [notifEnabled, setNotifEnabled] = useState(false);

  // ── Service Worker + PWA install ──────────────────────────────────────────
  useEffect(()=>{
    if('serviceWorker' in navigator) {
      navigator.serviceWorker.register('/sw.js').catch(()=>{});
    }
    setNotifEnabled(Notification?.permission === 'granted');
  },[]);

  const enableNotifications = async()=>{
    if(!('Notification' in window)) return;
    const perm = await Notification.requestPermission();
    setNotifEnabled(perm === 'granted');
  };

  const pushNotif = useCallback((title, body, tag='rf')=>{
    if(Notification?.permission === 'granted') {
      new Notification(title, { body, icon:'/logo.png', tag });
    }
  },[]);

  const fetchData = useCallback(async()=>{
    try {
      const [st,v,m,ek] = await Promise.all([sdk.getNetworkStats(), sdk.getVertices(), sdk.getMempool(), sdk.getRoundEk()]);
      setStats(st); setVertices(v||[]); setMempool(m||{count:0,txs:[]});
      setRoundEk(ek||{}); setOnline(true);
      // TPS history
      const cur = st?.consensus?.tx_count||0;
      const diff = cur - prevTx.current;
      prevTx.current = cur;
      const ts = new Date().toLocaleTimeString('en',{hour:'2-digit',minute:'2-digit',second:'2-digit'});
      setTpsHist(h=>[...h.slice(-29),{t:ts,v:Math.max(0,diff)}]);
      setPending(m?.count||0);
      sdk.get('/governance/proposals').then(r=>{
        setActiveProposals((r.proposals||[]).filter(p=>p.status==='active').length);
      }).catch(()=>{});
    } catch { setOnline(false); }
  },[]);

  useEffect(()=>{
    fetchData();
    const iv = setInterval(fetchData,15000); // fallback: WS drives updates
    return ()=>clearInterval(iv);
  },[fetchData]);

  // WebSocket — refresh stats + push notifications on network events
  useEffect(()=>{
    const unsub = sdk.connectWS(data=>{
      setWsData(data); fetchData();
      if(data?.type==='faucet' && wallet && data?.to && data.to.startsWith(wallet.address?.slice(0,16))) {
        pushNotif('Faucet received', `+${data.amount} RF arrived in your wallet`,'faucet');
      }
      if(data?.type==='staking_stake') {
        pushNotif('New validator', `${data.address?.slice(0,12)}… staked ${data.amount} RF`,'stake');
      }
    });
    return ()=>{ if(typeof unsub==='function') unsub(); };
  },[fetchData, wallet, pushNotif]);

  // Search redirect
  const handleSearch = e=>{
    e.preventDefault();
    if(search.trim()){ setSearchQuery(search.trim()); setPage('explorer'); }
  };

  if(!ks && !wallet)        return <Onboarding onComplete={w=>{ setWallet(w); setPage('wallet'); }}/>;
  if(ks  && !wallet)        return <LockScreen onUnlock={w=>{ setWallet(w); setPage('wallet'); }}/>;

  const pageTitles = {wallet:'My Wallet',dashboard:'Dashboard',staking:'Staking',bridge:'Bridge',dex:'DEX Trading',explorer:'Block Explorer',governance:'Governance',monitor:'Monitoring',docs:'Documentation',network:'Network Info',settings:'Settings'};

  const navTo = id => { setPage(id); setSidebarOpen(false); };

  return (
    <div className="shell">
      {/* MOBILE OVERLAY */}
      <div className={`sidebar-overlay ${sidebarOpen?'open':''}`} onClick={()=>setSidebarOpen(false)}/>

      {/* SIDEBAR */}
      <aside className={`sidebar ${sidebarOpen?'open':''}`}>
        <div className="sb-logo">
          <img src="./logo.png" alt="RF" style={{width:28,height:28,borderRadius:6,objectFit:'contain'}}/>
          redflag<span style={{fontWeight:300,opacity:.6}}>.web3</span>
        </div>
        <nav className="sb-nav">
          <div className="nav-lbl">Main</div>
          {PAGES.map(p=>(
            <div key={p.id} className={`nav-item ${page===p.id?'on':''}`} onClick={()=>navTo(p.id)}>
              <p.icon size={17}/>
              {p.label}
              {p.id==='wallet'     && pendingBadge>0    && <span className="nav-badge">{pendingBadge}</span>}
              {p.id==='governance' && activeProposals>0 && <span className="nav-badge">{activeProposals}</span>}
            </div>
          ))}
        </nav>
        <div className="sb-foot">
          <div style={{display:'flex',gap:8,justifyContent:'center',marginBottom:8}}>
            <a href="https://x.com/franff546758" target="_blank" rel="noreferrer"
              title="Twitter / X" style={{color:'var(--txl)',fontSize:11,textDecoration:'none'}}>𝕏</a>
            <span style={{color:'var(--bdr)'}}>·</span>
            <a href="https://t.me/redflag21blockchain" target="_blank" rel="noreferrer"
              title="Telegram" style={{color:'var(--txl)',fontSize:11,textDecoration:'none'}}>Telegram</a>
            <span style={{color:'var(--bdr)'}}>·</span>
            <a href="https://github.com/franklin0000/redflagweb3" target="_blank" rel="noreferrer"
              title="GitHub" style={{color:'var(--txl)',fontSize:11,textDecoration:'none'}}>GitHub</a>
          </div>
          <div className="node-pill">
            <div className={`ldot ${online?'on':'off'}`}/>
            <span className="node-pill-txt">{stats?.node?.peer_id ? short(stats.node.peer_id,12) : 'connecting…'}</span>
          </div>
        </div>
      </aside>

      {/* MAIN */}
      <div className="main">
        <div className="topbar">
          <button className="menu-toggle ibtn" onClick={()=>setSidebarOpen(o=>!o)} title="Menu">
            <Globe size={17}/>
          </button>
          <div className="topbar-ttl">{pageTitles[page]||page}</div>
          <form className="sw" onSubmit={handleSearch}>
            <Search size={14} className="si-ico"/>
            <input className="si" placeholder="Search address, tx, vertex…" value={search} onChange={e=>setSearch(e.target.value)}
              onKeyDown={e=>{ if(e.key==='Enter'){ setSearchQuery(search.trim()); setPage('explorer'); } }}/>
          </form>
          <div className="tb-right">
            <div style={{display:'flex',alignItems:'center',gap:7,fontSize:12,color:online?'var(--green)':'var(--txl)'}}>
              <div className={`ldot ${online?'on':'off'}`}/>
              {online?'Connected':'Offline'}
            </div>
            {!notifEnabled && Notification?.permission !== 'denied' && (
              <button className="ibtn" onClick={enableNotifications} title="Enable notifications" style={{color:'var(--yellow)'}}>
                <Activity size={14}/>
              </button>
            )}
            <button className="ibtn" onClick={fetchData} title="Refresh"><RefreshCw size={14}/></button>
          </div>
        </div>

        <div className="page">
          {page==='wallet'    && <WalletPage    wallet={wallet} wsData={wsData}/>}
          {page==='dashboard' && <DashboardPage stats={stats} vertices={vertices} mempool={mempool} roundEk={roundEk} tpsHist={tpsHist} online={online}/>}
          {page==='staking'   && <StakingPage   wallet={wallet} wsData={wsData}/>}
          {page==='bridge'    && <BridgePage    wallet={wallet}/>}
          {page==='dex'       && <DexPage       wallet={wallet} wsData={wsData}/>}
          {page==='explorer'  && <ExplorerPage  initialQuery={searchQuery} wsData={wsData}/>}
          {page==='governance'&& <GovernancePage wallet={wallet} wsData={wsData}/>}
          {page==='monitor'   && <MonitoringPage/>}
          {page==='docs'      && <DocsPage/>}
          {page==='network'   && <NetworkPage   stats={stats} online={online}/>}
          {page==='settings'  && <SettingsPage  wallet={wallet} onLogout={()=>{ setWallet(null); }}/>}
        </div>
      </div>
    </div>
  );
}
