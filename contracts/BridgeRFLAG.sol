// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

interface IERC20 {
    function transferFrom(address from, address to, uint256 amount) external returns (bool);
    function transfer(address to, uint256 amount) external returns (bool);
    function balanceOf(address account) external view returns (uint256);
    function allowance(address owner, address spender) external view returns (uint256);
}

// BridgeRFLAG - redflag.web3 ERC-20 Bridge
// Lock RFLAG on Polygon -> receive RF on redflag.web3
// Lock RF on redflag.web3 -> receive RFLAG on Polygon
contract BridgeRFLAG {

    event Locked(address indexed from, string rfAddress, uint256 amount, uint64 nonce);
    event Unlocked(address indexed to, uint256 amount, uint64 nonce);
    event RelayerUpdated(address indexed relayer, bool authorized);
    event Paused(address by);
    event Unpaused(address by);

    address public owner;
    IERC20  public immutable rflag;
    mapping(address => bool)  public authorizedRelayers;
    mapping(uint64  => bool)  public processedNonces;

    uint64  public lockNonce;
    bool    public paused;
    uint256 public dailyLimit;
    uint256 public dailyUnlocked;
    uint256 public lastResetDay;
    uint256 public feeBps;
    address public feeRecipient;

    bool private _lock;

    modifier onlyOwner()    { require(msg.sender == owner, "not owner"); _; }
    modifier onlyRelayer()  { require(authorizedRelayers[msg.sender], "not relayer"); _; }
    modifier whenNotPaused(){ require(!paused, "paused"); _; }
    modifier nonReentrant() { require(!_lock, "reentrant"); _lock = true; _; _lock = false; }

    constructor(address _rflag, address _relayer, uint256 _dailyLimit, uint256 _feeBps) {
        owner        = msg.sender;
        rflag        = IERC20(_rflag);
        authorizedRelayers[_relayer] = true;
        dailyLimit   = _dailyLimit;
        feeBps       = _feeBps;
        feeRecipient = msg.sender;
        lastResetDay = block.timestamp / 86400;
    }

    // Lock RFLAG to receive RF on redflag.web3
    function lock(string calldata rfAddress, uint256 amount)
        external whenNotPaused nonReentrant
    {
        require(amount > 0, "amount = 0");
        require(bytes(rfAddress).length >= 16, "rfAddress invalida");
        require(bytes(rfAddress).length <= 2048, "rfAddress muy larga");
        require(rflag.allowance(msg.sender, address(this)) >= amount, "allowance insuficiente");

        uint256 fee    = (amount * feeBps) / 10_000;
        uint256 netAmt = amount - fee;
        require(netAmt > 0, "amount muy pequeno");

        require(rflag.transferFrom(msg.sender, address(this), amount), "transfer failed");
        if (fee > 0) require(rflag.transfer(feeRecipient, fee), "fee failed");

        uint64 nonce = lockNonce++;
        emit Locked(msg.sender, rfAddress, netAmt, nonce);
    }

    // Relayer unlocks RFLAG after detecting RF lock on redflag.web3
    function unlock(address to, uint256 amount, uint64 nonce)
        external onlyRelayer whenNotPaused nonReentrant
    {
        require(to != address(0), "to = zero");
        require(amount > 0, "amount = 0");
        require(!processedNonces[nonce], "nonce procesado");
        require(rflag.balanceOf(address(this)) >= amount, "sin liquidez");

        uint256 today = block.timestamp / 86400;
        if (today > lastResetDay) { dailyUnlocked = 0; lastResetDay = today; }
        require(dailyUnlocked + amount <= dailyLimit, "limite diario");

        processedNonces[nonce] = true;
        dailyUnlocked += amount;

        require(rflag.transfer(to, amount), "transfer failed");
        emit Unlocked(to, amount, nonce);
    }

    function setRelayer(address relayer, bool auth) external onlyOwner {
        authorizedRelayers[relayer] = auth;
        emit RelayerUpdated(relayer, auth);
    }
    function setDailyLimit(uint256 l) external onlyOwner { dailyLimit = l; }
    function setFee(uint256 bps, address rec) external onlyOwner {
        require(bps <= 500, "max 5%"); feeBps = bps; feeRecipient = rec;
    }
    function pause()   external onlyOwner { paused = true;  emit Paused(msg.sender); }
    function unpause() external onlyOwner { paused = false; emit Unpaused(msg.sender); }
    function transferOwnership(address n) external onlyOwner { require(n != address(0)); owner = n; }
    function emergencyWithdraw(address to, uint256 amount) external onlyOwner {
        require(paused, "pause first");
        require(rflag.transfer(to, amount), "failed");
    }
    function bridgeBalance() external view returns (uint256) {
        return rflag.balanceOf(address(this));
    }
}
