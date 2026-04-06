// SPDX-License-Identifier: MIT
pragma solidity ^0.8.24;

/**
 * @title BridgeRF — RedFlag Cross-Chain Bridge Contract
 * @notice Permite hacer bridge de ETH/tokens nativos ↔ RF en la cadena RedFlag.
 *
 * Flujo EVM → RedFlag:
 *   1. Usuario llama a lock(rfAddress) con ETH/tokens
 *   2. Contrato emite evento Locked
 *   3. Relayer detecta evento y mintea RF en la cadena RedFlag
 *
 * Flujo RedFlag → EVM:
 *   1. Usuario bloquea RF en RedFlag (TX a BRIDGE_LOCK_ADDRESS con BridgeData en campo data)
 *   2. Relayer detecta TX y llama a unlock(to, amount, nonce) en este contrato
 *   3. Contrato transfiere ETH/tokens al destinatario EVM
 *
 * Seguridad:
 *   - processedNonces: evita replay attacks (cada nonce solo se procesa una vez)
 *   - dailyLimit: límite diario de withdrawals del relayer
 *   - pause/unpause: control de emergencia del owner
 *   - Multi-relayer: varios relayers pueden ejecutar unlock (requiere M de N en producción)
 */

/// @dev Importar OpenZeppelin en producción (via npm/hardhat):
/// import "@openzeppelin/contracts/access/Ownable.sol";
/// import "@openzeppelin/contracts/security/ReentrancyGuard.sol";
/// import "@openzeppelin/contracts/security/Pausable.sol";
/// Por ahora implementamos inline para no requerir dependencias externas.

contract BridgeRF {

    // ── Eventos ────────────────────────────────────────────────────────────────

    /// Emitido cuando un usuario bloquea ETH para recibirlo en RedFlag
    event Locked(
        address indexed from,
        string  rfAddress,
        uint256 amount,
        uint64  nonce
    );

    /// Emitido cuando el relayer libera ETH a un usuario desde RedFlag
    event Unlocked(
        address indexed to,
        uint256 amount,
        uint64  nonce
    );

    event RelayerUpdated(address indexed relayer, bool authorized);
    event DailyLimitUpdated(uint256 newLimit);
    event Paused(address by);
    event Unpaused(address by);
    event EmergencyWithdraw(address to, uint256 amount);

    // ── Estado ─────────────────────────────────────────────────────────────────

    address public owner;
    mapping(address => bool) public authorizedRelayers;

    /// Nonces RF→EVM ya procesados (evita replay)
    mapping(uint64 => bool) public processedNonces;

    /// Nonce incremental para bloqueos EVM→RF (lock nonce)
    uint64 public lockNonce;

    /// Balance total bloqueado en el contrato
    uint256 public lockedBalance;

    /// Pausa de emergencia
    bool public paused;

    /// Límite diario de unlock (seguridad anti-hack)
    uint256 public dailyLimit;
    uint256 public dailyUnlocked;
    uint256 public lastResetDay;

    /// Comisión del bridge (en basis points, 100 = 1%)
    uint256 public feeBps;  // default 10 = 0.1%
    address public feeRecipient;

    // ── Constructor ────────────────────────────────────────────────────────────

    constructor(address _relayer, uint256 _dailyLimit, uint256 _feeBps) {
        owner = msg.sender;
        authorizedRelayers[_relayer] = true;
        dailyLimit  = _dailyLimit;
        feeBps      = _feeBps;
        feeRecipient = msg.sender;
        lastResetDay = block.timestamp / 86400;
    }

    // ── Modificadores ──────────────────────────────────────────────────────────

    modifier onlyOwner()    { require(msg.sender == owner,    "BridgeRF: not owner");   _; }
    modifier onlyRelayer()  { require(authorizedRelayers[msg.sender], "BridgeRF: not relayer"); _; }
    modifier whenNotPaused(){ require(!paused, "BridgeRF: paused"); _; }
    modifier nonReentrant() {
        require(!_reentrancyLock, "BridgeRF: reentrant");
        _reentrancyLock = true;
        _;
        _reentrancyLock = false;
    }
    bool private _reentrancyLock;

    // ── Función principal: lock ETH → RF ───────────────────────────────────────

    /**
     * @notice Bloquea ETH y emite evento para mintear RF en la cadena RedFlag.
     * @param rfAddress Dirección ML-DSA en la cadena RedFlag que recibirá los RF.
     */
    function lock(string calldata rfAddress) external payable whenNotPaused nonReentrant {
        require(msg.value > 0,       "BridgeRF: amount = 0");
        require(bytes(rfAddress).length >= 16, "BridgeRF: invalid rfAddress");
        require(bytes(rfAddress).length <= 2048, "BridgeRF: rfAddress too long");

        // Calcular fee
        uint256 fee    = (msg.value * feeBps) / 10_000;
        uint256 netAmt = msg.value - fee;
        require(netAmt > 0, "BridgeRF: amount too small after fee");

        // Transferir fee al recipient
        if (fee > 0) {
            (bool ok,) = feeRecipient.call{value: fee}("");
            require(ok, "BridgeRF: fee transfer failed");
        }

        lockedBalance += netAmt;
        uint64 nonce = lockNonce++;

        emit Locked(msg.sender, rfAddress, netAmt, nonce);
    }

    // ── Función del relayer: unlock ETH ← RF ──────────────────────────────────

    /**
     * @notice Libera ETH al destinatario EVM tras detectar un lock en RedFlag.
     * @dev Solo ejecutable por relayers autorizados. Protegido contra replay con nonce.
     * @param to     Dirección EVM destino.
     * @param amount Cantidad en wei (1 RF = 1e12 wei en este bridge).
     * @param nonce  Nonce único de la TX en RedFlag.
     */
    function unlock(
        address payable to,
        uint256 amount,
        uint64  nonce
    ) external onlyRelayer whenNotPaused nonReentrant {
        require(to      != address(0),    "BridgeRF: to = zero");
        require(amount  > 0,              "BridgeRF: amount = 0");
        require(!processedNonces[nonce],  "BridgeRF: nonce ya procesado");
        require(lockedBalance >= amount,  "BridgeRF: fondos insuficientes");

        // Reset diario
        uint256 today = block.timestamp / 86400;
        if (today > lastResetDay) {
            dailyUnlocked = 0;
            lastResetDay  = today;
        }

        // Límite diario
        require(dailyUnlocked + amount <= dailyLimit, "BridgeRF: limite diario alcanzado");

        processedNonces[nonce] = true;
        lockedBalance -= amount;
        dailyUnlocked += amount;

        (bool success,) = to.call{value: amount}("");
        require(success, "BridgeRF: transfer failed");

        emit Unlocked(to, amount, nonce);
    }

    // ── Admin ──────────────────────────────────────────────────────────────────

    function setRelayer(address relayer, bool authorized) external onlyOwner {
        authorizedRelayers[relayer] = authorized;
        emit RelayerUpdated(relayer, authorized);
    }

    function setDailyLimit(uint256 newLimit) external onlyOwner {
        dailyLimit = newLimit;
        emit DailyLimitUpdated(newLimit);
    }

    function setFee(uint256 _feeBps, address _recipient) external onlyOwner {
        require(_feeBps <= 500, "BridgeRF: fee max 5%");
        feeBps       = _feeBps;
        feeRecipient = _recipient;
    }

    function pause()   external onlyOwner { paused = true;  emit Paused(msg.sender); }
    function unpause() external onlyOwner { paused = false; emit Unpaused(msg.sender); }

    function transferOwnership(address newOwner) external onlyOwner {
        require(newOwner != address(0), "BridgeRF: new owner = zero");
        owner = newOwner;
    }

    /// Retiro de emergencia (solo owner, solo cuando está pausado)
    function emergencyWithdraw(address payable to, uint256 amount) external onlyOwner {
        require(paused, "BridgeRF: pause first");
        require(amount <= address(this).balance, "BridgeRF: insufficient balance");
        (bool ok,) = to.call{value: amount}("");
        require(ok, "BridgeRF: withdraw failed");
        emit EmergencyWithdraw(to, amount);
    }

    // ── Views ──────────────────────────────────────────────────────────────────

    function contractBalance() external view returns (uint256) {
        return address(this).balance;
    }

    function isNonceProcessed(uint64 nonce) external view returns (bool) {
        return processedNonces[nonce];
    }

    receive() external payable {}
}
