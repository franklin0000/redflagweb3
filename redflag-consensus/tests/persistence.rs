use redflag_consensus::{Dag, Vertex};
use std::collections::HashSet;

#[test]
fn test_dag_persistence_on_reboot() -> Result<(), Box<dyn std::error::Error>> {
    let db_path = "./test_persistence_db";
    let _ = std::fs::remove_dir_all(db_path);

    let v1_id;
    {
        // 1. Iniciar DAG y guardar un vértice
        let dag = Dag::new(db_path)?;
        let v1 = Vertex {
            round: 1,
            parents: HashSet::new(),
            transactions: vec![],
            encrypted_transactions: vec![],
            author: vec![123],
            signature: vec![45, 67],
        };
        v1_id = v1.id();
        dag.insert_vertex(v1)?;
        println!("✅ Vértice guardado en DB.");
    } // El DAG se cierra aquí (Scope drop)

    {
        // 2. "Reiniciar" el DAG cargando desde la misma ruta
        let dag = Dag::new(db_path)?;
        let v_recovered = dag.get_vertex(&v1_id);
        
        assert!(v_recovered.is_some(), "El vértice debería persistir después del reinicio");
        let v = v_recovered.unwrap();
        assert_eq!(v.round, 1);
        assert_eq!(v.author, vec![123]);
        println!("🎊 Vértice recuperado con éxito desde el disco.");
    }

    Ok(())
}
